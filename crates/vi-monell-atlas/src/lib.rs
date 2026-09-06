//! Pattern-and-practice (Monell) atlas: substantiated constitutional findings
//! only. Reports are attorney work product; flags never auto-publish.
#![forbid(unsafe_code)]

pub mod report;
pub mod stats;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

pub const EVENT: &str = vi_ledger::events::CONSTITUTIONAL_FINDING;

const VALID_TYPES: &[&str] = &[
    "brady",
    "giglio",
    "batson",
    "discovery",
    "witness_subornation",
    "sanction",
    "due_process",
    "other",
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("invalid finding_type: {0}")]
    InvalidType(String),
    #[error("invalid review status: {0}")]
    InvalidStatus(String),
    #[error("finding not found")]
    NotFound,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FindingInput {
    pub case_id: Option<Uuid>,
    pub prosecutor_id: Option<Uuid>,
    /// The individual this finding concerns, in any role.
    ///
    /// Without this, a finding could only ever name a seeded prosecutor, and a
    /// judge who abandoned the record would have nowhere to be recorded.
    pub actor_id: Option<Uuid>,
    pub office: String,
    pub jurisdiction: String,
    pub finding_type: String,
    pub court_level: Option<String>,
    pub judge: Option<String>,
    pub finding_date: Option<NaiveDate>,
    pub source_citation: String,
    pub source_url: Option<String>,
    pub summary: String,
}

pub fn validate_type(t: &str) -> Result<(), Error> {
    if VALID_TYPES.contains(&t) {
        Ok(())
    } else {
        Err(Error::InvalidType(t.to_string()))
    }
}

pub async fn record_finding(
    pool: &PgPool,
    ledger: &Ledger,
    input: &FindingInput,
) -> Result<Uuid, Error> {
    validate_type(&input.finding_type)?;
    let id = Uuid::new_v4();
    let source_hash = vi_ledger::hash_payload(&json!(input));

    sqlx::query(
        "INSERT INTO constitutional_findings
         (finding_id, case_id, prosecutor_id, actor_id, office, jurisdiction, finding_type,
          court_level, judge, finding_date, source_citation, source_url, summary, document_hash)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(id)
    .bind(input.case_id)
    .bind(input.prosecutor_id)
    .bind(input.actor_id)
    .bind(&input.office)
    .bind(&input.jurisdiction)
    .bind(&input.finding_type)
    .bind(&input.court_level)
    .bind(&input.judge)
    .bind(input.finding_date)
    .bind(&input.source_citation)
    .bind(&input.source_url)
    .bind(&input.summary)
    .bind(&source_hash)
    .execute(pool)
    .await?;

    ledger
        .append(
            EVENT,
            &json!({
                "finding_id": id,
                "actor_id": input.actor_id,
                "office": input.office,
                "jurisdiction": input.jurisdiction,
                "finding_type": input.finding_type,
                "source_hash": source_hash,
            }),
        )
        .await?;

    Ok(id)
}

pub async fn review_finding(
    pool: &PgPool,
    ledger: &Ledger,
    finding_id: Uuid,
    status: &str,
    reviewer: Option<Uuid>,
) -> Result<(), Error> {
    if !matches!(status, "substantiated" | "rejected") {
        return Err(Error::InvalidStatus(status.to_string()));
    }
    let res = sqlx::query(
        "UPDATE constitutional_findings
         SET review_status=$1, reviewed_by=$2, reviewed_at=now()
         WHERE finding_id=$3",
    )
    .bind(status)
    .bind(reviewer)
    .bind(finding_id)
    .execute(pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    ledger
        .append(
            vi_ledger::events::FINDING_REVIEWED,
            &json!({ "finding_id": finding_id, "status": status }),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_validation() {
        assert!(validate_type("brady").is_ok());
        assert!(validate_type("not-a-type").is_err());
    }
}
