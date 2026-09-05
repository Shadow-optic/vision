//! Legal-action generator. Output is attorney work product for licensed
//! counsel. The engine does not file charges, complaints, or motions.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

use crate::entity::Actor;
use crate::report;
use crate::score::{self, AbuseScore};
use crate::sentencing::{self, SentenceAdvocacy};
use crate::statutes::{self, ImmunityNote, Statute};
use crate::Error;

pub const KINDS: &[&str] = &[
    "criminal_referral",
    "civil_1983",
    "bar_complaint",
    "sentencing_memo",
];

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StoredPackage {
    pub package_id: Uuid,
    pub actor_id: Uuid,
    pub action_kind: String,
    pub status: String,
    pub body_markdown: String,
    pub document_hash: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EvidenceRow {
    pub source_kind: String,
    pub label: String,
    pub citation: Option<String>,
    pub summary: String,
    pub case_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct PackageContext {
    pub actor: Actor,
    pub score: AbuseScore,
    pub action_kind: String,
    pub evidence: Vec<EvidenceRow>,
    pub statutes: Vec<&'static Statute>,
    pub immunity: &'static [ImmunityNote],
    pub destinations: Vec<&'static str>,
    pub ledger: Vec<crate::report::LedgerRef>,
    pub advocacy: SentenceAdvocacy,
}

pub fn validate_kind(kind: &str) -> Result<(), Error> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(Error::InvalidKind(kind.to_string()))
    }
}

pub async fn evidence_for_actor(pool: &PgPool, actor: &Actor) -> Result<Vec<EvidenceRow>, Error> {
    let mut rows = Vec::new();
    if let Some(pid) = actor.prosecutor_id {
        let findings: Vec<EvidenceRow> = sqlx::query_as(
            "SELECT 'constitutional_finding'::text AS source_kind,
                    finding_type AS label,
                    source_citation AS citation,
                    summary,
                    case_id
             FROM constitutional_findings
             WHERE prosecutor_id = $1 AND review_status = 'substantiated'
             ORDER BY finding_date NULLS LAST, created_at",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        rows.extend(findings);

        let flags: Vec<EvidenceRow> = sqlx::query_as(
            "SELECT 'abuse_flag'::text AS source_kind,
                    label,
                    NULL::text AS citation,
                    COALESCE(explanation::text, '') AS summary,
                    case_id
             FROM abuse_flags
             WHERE prosecutor_id = $1 AND review_status = 'substantiated'
             ORDER BY created_at",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?;
        rows.extend(flags);
    }
    Ok(rows)
}

fn destinations(kind: &str, actor: &Actor) -> Vec<&'static str> {
    match kind {
        "criminal_referral" => vec![
            "State Attorney General (criminal division)",
            "United States Attorney for the district of the underlying case",
            "U.S. Department of Justice, Civil Rights Division, Criminal Section",
        ],
        "civil_1983" => vec![
            "Licensed civil-rights counsel (plaintiff-side)",
            "State tort claims where a damages cap or notice statute applies",
        ],
        "bar_complaint" => {
            if actor.jurisdiction.eq_ignore_ascii_case("CA") {
                vec!["State Bar of California, Office of Chief Trial Counsel"]
            } else {
                vec!["State bar disciplinary authority for the actor's jurisdiction"]
            }
        }
        "sentencing_memo" => {
            vec!["Counsel of record — file after conviction; seek the statutory maximum, including life where authorized"]
        }
        _ => vec!["Licensed counsel"],
    }
}

async fn aggravators(pool: &PgPool, prosecutor_id: Option<Uuid>) -> Result<(bool, bool), Error> {
    let Some(pid) = prosecutor_id else {
        return Ok((false, false));
    };
    let row: (bool, bool) = sqlx::query_as(
        "SELECT COALESCE(BOOL_OR(death_resulted), false),
                COALESCE(BOOL_OR(bodily_injury), false)
         FROM constitutional_findings
         WHERE prosecutor_id = $1 AND review_status = 'substantiated'",
    )
    .bind(pid)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn generate(
    pool: &PgPool,
    ledger: &Ledger,
    actor_id: Uuid,
    kind: &str,
) -> Result<(StoredPackage, String), Error> {
    validate_kind(kind)?;
    let actor = crate::entity::get(pool, actor_id).await?;
    let evidence = evidence_for_actor(pool, &actor).await?;
    if evidence.is_empty() {
        return Err(Error::InsufficientEvidence);
    }
    let score = score::score_actor(pool, Some(ledger), actor_id).await?;
    let types: Vec<String> = evidence.iter().map(|e| e.label.clone()).collect();
    let statutes = statutes::for_finding_types(&types);
    let ledger_rows = ledger.entries_for_actor(actor_id).await?;
    let ledger_refs: Vec<crate::report::LedgerRef> = ledger_rows
        .into_iter()
        .map(|e| crate::report::LedgerRef {
            seq: e.seq,
            event_type: e.event_type,
            entry_hash: e.entry_hash,
        })
        .collect();

    let (death_resulted, bodily_injury) = aggravators(pool, actor.prosecutor_id).await?;
    let advocacy = sentencing::assemble(&statutes, death_resulted, bodily_injury);
    let destinations = destinations(kind, &actor);
    let ctx = PackageContext {
        actor,
        score,
        action_kind: kind.to_string(),
        evidence,
        statutes,
        immunity: statutes::IMMUNITY,
        destinations,
        ledger: ledger_refs,
        advocacy,
    };

    let md = report::render_package(&ctx)?;
    let payload = json!({
        "actor_id": actor_id,
        "action_kind": kind,
        "score": ctx.score.score,
        "evidence_count": ctx.evidence.len(),
        "statute_citations": ctx.statutes.iter().map(|s| s.citation).collect::<Vec<_>>(),
        "life_available": ctx.advocacy.life_available,
        "advocated_sentence": ctx.advocacy.advocated_sentence,
    });
    let document_hash = vi_ledger::hash_payload(&json!({"markdown": md, "payload": payload}));
    let package_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO legal_action_packages
         (package_id, actor_id, action_kind, status, body_markdown, payload, document_hash)
         VALUES ($1,$2,$3,'draft',$4,$5,$6)",
    )
    .bind(package_id)
    .bind(actor_id)
    .bind(kind)
    .bind(&md)
    .bind(&payload)
    .bind(&document_hash)
    .execute(pool)
    .await?;

    ledger
        .append(
            vi_ledger::events::LEGAL_PACKAGE,
            &json!({
                "actor_id": actor_id,
                "package_id": package_id,
                "action_kind": kind,
                "document_hash": document_hash,
            }),
        )
        .await?;

    Ok((
        StoredPackage {
            package_id,
            actor_id,
            action_kind: kind.to_string(),
            status: "draft".into(),
            body_markdown: md.clone(),
            document_hash,
        },
        md,
    ))
}

pub async fn get_package(pool: &PgPool, package_id: Uuid) -> Result<StoredPackage, Error> {
    sqlx::query_as::<_, StoredPackage>(
        "SELECT package_id, actor_id, action_kind, status, body_markdown, document_hash
         FROM legal_action_packages WHERE package_id = $1",
    )
    .bind(package_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound)
}

pub async fn list_packages(
    pool: &PgPool,
    actor_id: Option<Uuid>,
    kind: Option<&str>,
) -> Result<Vec<StoredPackage>, Error> {
    Ok(sqlx::query_as::<_, StoredPackage>(
        "SELECT package_id, actor_id, action_kind, status, body_markdown, document_hash
         FROM legal_action_packages
         WHERE ($1::uuid IS NULL OR actor_id = $1)
           AND ($2::text IS NULL OR action_kind = $2)
         ORDER BY created_at DESC",
    )
    .bind(actor_id)
    .bind(kind)
    .fetch_all(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_validation() {
        assert!(validate_kind("criminal_referral").is_ok());
        assert!(validate_kind("life_for_life").is_err());
    }
}
