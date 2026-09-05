//! Root Ledger: append-only, hash-chained, tamper-evident event log.
//! entry_hash = BLAKE3( "VI-Ledger/v1" || prev_hash || payload_hash || ts_micros )
#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger verification failed at seq {0}")]
    Corrupt(i64),
}

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const DOMAIN: &[u8] = b"VI-Ledger/v1";

pub mod events {
    pub const CASE_INGESTED: &str = "CaseIngested";
    pub const CASE_DISPOSITION: &str = "CaseDisposition";
    pub const PROSECUTOR_ACTION: &str = "ProsecutorAction";
    pub const BRADY_FLAG: &str = "BradyViolationFlag";
    pub const ABUSE_FLAG: &str = "AbuseFlag";
    pub const SIMULATION_RESULT: &str = "SimulationResult";
    pub const RULE_CREATED: &str = "RuleCreated";
    pub const CORRECTION_ISSUED: &str = "CorrectionIssued";
    pub const CONSTITUTIONAL_FINDING: &str = "ConstitutionalFinding";
    pub const FINDING_REVIEWED: &str = "FindingReviewed";
    pub const DISCLOSED_EVIDENCE: &str = "DisclosedEvidenceItem";
    pub const BRADY_RECON: &str = "BradyReconRun";
    pub const TRIAL_PENALTY_SNAPSHOT: &str = "TrialPenaltySnapshot";
    pub const TACTIC_RECORDED: &str = "TacticRecorded";
    pub const OPINION_INGESTED: &str = "OpinionIngested";
    pub const CONSTITUTION_SCREEN_RUN: &str = "ConstitutionScreenRun";
    pub const ACTOR_RESOLVED: &str = "ActorResolved";
    pub const ABUSE_SCORE: &str = "AbuseScoreComputed";
    pub const LEGAL_PACKAGE: &str = "LegalActionPackage";
    pub const PUBLICATION_REVIEWED: &str = "PublicationReviewed";
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LedgerEntry {
    pub seq: i64,
    pub id: Uuid,
    pub event_type: String,
    pub payload: Value,
    pub payload_hash: String,
    pub prev_hash: String,
    pub entry_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct VerifyReport {
    pub entries: u64,
    pub ok: bool,
    pub first_bad_seq: Option<i64>,
    pub tip_hash: Option<String>,
}

/// serde_json maps are BTreeMaps (feature `preserve_order` off), so
/// serialization is key-ordered and deterministic.
pub fn hash_payload(payload: &Value) -> String {
    let bytes = serde_json::to_vec(payload).expect("Value always serializes");
    blake3::hash(&bytes).to_hex().to_string()
}

pub fn compute_entry_hash(prev_hash: &str, payload_hash: &str, ts_micros: i64) -> String {
    let mut h = blake3::Hasher::new();
    h.update(DOMAIN);
    h.update(prev_hash.as_bytes());
    h.update(payload_hash.as_bytes());
    h.update(&ts_micros.to_be_bytes());
    h.finalize().to_hex().to_string()
}

#[derive(Clone)]
pub struct Ledger {
    pool: PgPool,
}

impl Ledger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Appends one entry. Writers serialize on the tail row via SELECT ... FOR
    /// UPDATE — correct at MVP throughput. If single-writer contention ever
    /// matters, move sequencing behind one async task (the "sequencer pattern").
    pub async fn append(&self, event_type: &str, payload: &Value) -> Result<LedgerEntry, Error> {
        let payload_hash = hash_payload(payload);
        // Hash microseconds, not a string: Postgres timestamptz round-trips at
        // microsecond precision, so verification is exact.
        let ts_micros = Utc::now().timestamp_micros();
        let created_at = DateTime::from_timestamp_micros(ts_micros).expect("in range");

        let mut tx = self.pool.begin().await?;
        // Advisory lock serializes tip-read + insert. FOR UPDATE on the current
        // tail row is not enough: a waiter can re-lock the old tip after a
        // concurrent insert commits and fork the chain.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(0x5649_4C45_4447i64)
            .execute(&mut *tx)
            .await?;
        let prev_hash: String =
            sqlx::query_scalar("SELECT entry_hash FROM ledger_entries ORDER BY seq DESC LIMIT 1")
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or_else(|| GENESIS.to_string());

        let entry_hash = compute_entry_hash(&prev_hash, &payload_hash, ts_micros);
        let id = Uuid::new_v4();

        let entry = sqlx::query_as::<_, LedgerEntry>(
            "INSERT INTO ledger_entries
               (id, event_type, payload, payload_hash, prev_hash, entry_hash, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             RETURNING seq, id, event_type, payload, payload_hash, prev_hash, entry_hash, created_at",
        )
        .bind(id)
        .bind(event_type)
        .bind(payload)
        .bind(&payload_hash)
        .bind(&prev_hash)
        .bind(&entry_hash)
        .bind(created_at)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(entry)
    }

    pub async fn entries_for_case(&self, case_id: Uuid) -> Result<Vec<LedgerEntry>, Error> {
        let rows = sqlx::query_as::<_, LedgerEntry>(
            "SELECT seq,id,event_type,payload,payload_hash,prev_hash,entry_hash,created_at
             FROM ledger_entries
             WHERE payload->>'case_id' = $1
             ORDER BY seq ASC",
        )
        .bind(case_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn entries_for_actor(&self, actor_id: Uuid) -> Result<Vec<LedgerEntry>, Error> {
        let rows = sqlx::query_as::<_, LedgerEntry>(
            "SELECT seq,id,event_type,payload,payload_hash,prev_hash,entry_hash,created_at
             FROM ledger_entries
             WHERE payload->>'actor_id' = $1
             ORDER BY seq ASC",
        )
        .bind(actor_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Full chain audit. O(n); intended to run on a schedule and on demand.
    pub async fn verify(&self) -> Result<VerifyReport, Error> {
        let rows = sqlx::query_as::<_, LedgerEntry>(
            "SELECT seq,id,event_type,payload,payload_hash,prev_hash,entry_hash,created_at
             FROM ledger_entries ORDER BY seq ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut prev = GENESIS.to_string();
        let mut report = VerifyReport {
            entries: rows.len() as u64,
            ok: true,
            first_bad_seq: None,
            tip_hash: rows.last().map(|r| r.entry_hash.clone()),
        };
        for r in &rows {
            let chain_ok = r.prev_hash == prev
                && compute_entry_hash(
                    &r.prev_hash,
                    &r.payload_hash,
                    r.created_at.timestamp_micros(),
                ) == r.entry_hash
                && hash_payload(&r.payload) == r.payload_hash;
            if !chain_ok {
                report.ok = false;
                report.first_bad_seq = Some(r.seq);
                return Ok(report);
            }
            prev = r.entry_hash.clone();
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn payload_hash_is_key_order_independent() {
        let a = json!({"x": 1, "y": [1,2,3], "z": {"p": true, "q": null}});
        let b = json!({"z": {"q": null, "p": true}, "y": [1,2,3], "x": 1});
        assert_eq!(hash_payload(&a), hash_payload(&b));
    }

    #[test]
    fn chain_is_deterministic() {
        let h1 = compute_entry_hash(GENESIS, "aaa", 1_700_000_000_000_000);
        let h1b = compute_entry_hash(GENESIS, "aaa", 1_700_000_000_000_000);
        assert_eq!(h1, h1b);
        let h2 = compute_entry_hash(&h1, "bbb", 1_700_000_001_000_000);
        assert_ne!(h1, h2);
        assert_eq!(h2.len(), 64);
    }

    #[test]
    fn domain_separation_changes_hash() {
        let mut h = blake3::Hasher::new();
        h.update(b"other");
        h.update(GENESIS.as_bytes());
        h.update(b"aaa");
        h.update(&1_i64.to_be_bytes());
        let other = h.finalize().to_hex().to_string();
        assert_ne!(compute_entry_hash(GENESIS, "aaa", 1), other);
    }
}
