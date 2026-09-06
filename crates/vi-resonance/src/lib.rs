//! Weak-Signal Fusion Resonance — a meta-engine that fuses sub-threshold
//! signals across engines per case into one composite score, surfacing cases
//! no single engine flags.
//!
//! Every row produced is machine-derived, advisory, and `pending` counsel
//! review. A resonance score is a research lead, never a finding. No data is
//! fabricated: an empty corpus yields an empty report.
//!
//! Method: each signal → one-sided p-value against its corpus distribution
//! (empirical CDF with a min-count guard) → combined via Fisher's χ² AND
//! weighted Stouffer's Z (both reported) → Benjamini–Hochberg q-values across
//! all scored cases.
#![forbid(unsafe_code)]

pub mod signals;
pub mod stats;

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

/// Ledger event type for one full recompute run.
pub const EVENT_COMPUTED: &str = "ResonanceComputed";
/// Cases at or below this BH q-value count as `surfaced` in the report.
pub const SURFACE_Q: f64 = 0.25;

pub const FORMULA: &str = "p_i = (1 + #{x ≥ v_i})/(n_i + 1) per-signal empirical CDF \
    (min corpus 8); Fisher χ² = −2Σ ln p_i (df = 2k); \
    Stouffer Z = Σ w_i Φ⁻¹(1−p_i)/√(Σ w_i²), weights abuse 1.5 / constitution 1.0 \
    / brady 1.0 / geo 0.5; q = Benjamini–Hochberg over all scored cases";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct ResonanceReport {
    pub scored: usize,
    pub surfaced: usize,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CaseResonance {
    pub case_id: Uuid,
    pub fisher_chi2: f64,
    pub fisher_p: f64,
    pub stouffer_z: f64,
    pub q_value: f64,
    pub n_signals: i32,
    pub signals: Value,
    pub computed_at: DateTime<Utc>,
    pub status: String,
}

struct ScoredRow {
    case_id: Uuid,
    fisher_chi2: f64,
    fisher_p: f64,
    stouffer_z: f64,
    n_signals: usize,
    signals: Value,
}

/// Pure combination core, separated from DB for testability: given per-case
/// signals and per-signal corpora, produce scored rows (q-values filled in by
/// the caller across the full case set).
fn score_cases(
    per_case: &BTreeMap<Uuid, Vec<signals::RawSignal>>,
    corpora: &HashMap<&'static str, Vec<f64>>,
) -> Vec<ScoredRow> {
    let mut rows = Vec::new();
    for (case_id, sigs) in per_case {
        let mut ps = Vec::new();
        let mut ws = Vec::new();
        let mut items = Vec::new();
        for s in sigs {
            let corpus = &corpora[s.name];
            let Some(p) = stats::empirical_p_upper(s.value, corpus) else {
                continue; // min-count guard: signal dropped corpus-wide
            };
            ps.push(p);
            ws.push(s.weight);
            items.push(json!({
                "name": s.name,
                "value": s.value,
                "corpus_n": corpus.len(),
                "p": p,
                "weight": s.weight,
            }));
        }
        if ps.is_empty() {
            continue;
        }
        let (fisher_chi2, fisher_p) = stats::fisher_combine(&ps).expect("non-empty");
        let stouffer_z = stats::stouffer_combine(&ps, &ws).expect("non-empty");
        rows.push(ScoredRow {
            case_id: *case_id,
            fisher_chi2,
            fisher_p,
            stouffer_z,
            n_signals: ps.len(),
            signals: json!({
                "machine_derived": true,
                "formula": FORMULA,
                "items": items,
            }),
        });
    }
    rows
}

/// Recompute resonance for every case with at least one surviving signal.
/// Fully replaces `case_resonance` (pending artifacts, recomputed wholesale);
/// appends one ledger event per run. Empty corpus → empty report, no rows.
pub async fn compute_all(pool: &PgPool) -> Result<ResonanceReport, Error> {
    let raw = signals::extract_signals(pool).await?;

    let mut per_case: BTreeMap<Uuid, Vec<signals::RawSignal>> = BTreeMap::new();
    let mut corpora: HashMap<&'static str, Vec<f64>> = HashMap::new();
    for s in raw {
        corpora.entry(s.name).or_default().push(s.value);
        per_case.entry(s.case_id).or_default().push(s);
    }

    let mut rows = score_cases(&per_case, &corpora);
    let qs = stats::benjamini_hochberg(&rows.iter().map(|r| r.fisher_p).collect::<Vec<_>>());
    let surfaced = qs.iter().filter(|&&q| q <= SURFACE_Q).count();

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM case_resonance")
        .execute(&mut *tx)
        .await?;
    for (row, q) in rows.drain(..).zip(qs.iter()) {
        sqlx::query(
            "INSERT INTO case_resonance
               (case_id, fisher_chi2, fisher_p, stouffer_z, q_value, n_signals, signals)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(row.case_id)
        .bind(row.fisher_chi2)
        .bind(row.fisher_p)
        .bind(row.stouffer_z)
        .bind(*q)
        .bind(row.n_signals as i32)
        .bind(&row.signals)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let report = ResonanceReport {
        scored: qs.len(),
        surfaced,
    };
    Ledger::new(pool.clone())
        .append(
            EVENT_COMPUTED,
            &json!({
                "scored": report.scored,
                "surfaced": report.surfaced,
                "surface_q": SURFACE_Q,
                "machine_derived": true,
            }),
        )
        .await?;
    Ok(report)
}

/// One case's resonance row, with the full per-signal breakdown.
pub async fn case_detail(
    pool: &PgPool,
    case_id: Uuid,
) -> Result<Option<CaseResonance>, Error> {
    Ok(sqlx::query_as::<_, CaseResonance>(
        "SELECT case_id, fisher_chi2, fisher_p, stouffer_z, q_value, n_signals,
                signals, computed_at, status
           FROM case_resonance WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::RawSignal;

    fn sig(case: Uuid, name: &'static str, value: f64, weight: f64) -> RawSignal {
        RawSignal {
            case_id: case,
            name,
            value,
            weight,
        }
    }

    #[test]
    fn min_count_guard_drops_thin_signal_for_every_case() {
        // Only 5 cases carry the abuse signal (< MIN_CORPUS 8) → no rows.
        let mut per_case = BTreeMap::new();
        let mut corpus = Vec::new();
        for i in 0..5 {
            let id = Uuid::from_u128(i);
            per_case.insert(id, vec![sig(id, "abuse", 3.0, 1.5)]);
            corpus.push(3.0);
        }
        let mut corpora = HashMap::new();
        corpora.insert("abuse", corpus);
        assert!(score_cases(&per_case, &corpora).is_empty());
    }

    #[test]
    fn empty_corpus_scores_nothing() {
        let per_case = BTreeMap::new();
        let corpora = HashMap::new();
        assert!(score_cases(&per_case, &corpora).is_empty());
    }

    #[test]
    fn highest_signal_case_gets_smallest_p() {
        // 8 cases, one extreme value. p for v=8 is (1+1)/9 = 2/9;
        // single-signal Fisher returns the p itself.
        let mut per_case = BTreeMap::new();
        let mut corpus = Vec::new();
        for i in 1..=8u128 {
            let id = Uuid::from_u128(i);
            let v = i as f64;
            per_case.insert(id, vec![sig(id, "brady", v, 1.0)]);
            corpus.push(v);
        }
        let mut corpora = HashMap::new();
        corpora.insert("brady", corpus);
        let rows = score_cases(&per_case, &corpora);
        assert_eq!(rows.len(), 8);
        let top = rows
            .iter()
            .find(|r| r.case_id == Uuid::from_u128(8))
            .unwrap();
        assert!((top.fisher_p - 2.0 / 9.0).abs() < 1e-12);
        assert_eq!(top.n_signals, 1);
        assert_eq!(top.signals["machine_derived"], json!(true));
        let bottom = rows
            .iter()
            .find(|r| r.case_id == Uuid::from_u128(1))
            .unwrap();
        assert!((bottom.fisher_p - 1.0).abs() < 1e-12);
        assert!(top.fisher_p < bottom.fisher_p);
        assert!(top.stouffer_z > bottom.stouffer_z);
    }
}
