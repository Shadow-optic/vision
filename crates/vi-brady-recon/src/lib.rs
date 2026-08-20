//! Exculpatory-evidence reconciliation: expected items (from public opinions)
//! vs disclosed items. Gaps are research leads, never auto-published Brady findings.
#![forbid(unsafe_code)]

pub mod extractor;
pub mod reconcile;
pub mod report;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

pub const EVENT_DISCLOSED: &str = vi_ledger::events::DISCLOSED_EVIDENCE;
pub const EVENT_RECON: &str = vi_ledger::events::BRADY_RECON;

const VALID_TYPES: &[&str] = &[
    "witness_interview",
    "bodycam",
    "lab_report",
    "chain_of_custody",
    "informant_benefit",
    "911_call",
    "forensic_worksheet",
    "other",
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("invalid evidence item_type: {0}")]
    InvalidType(String),
}

#[derive(Debug, Deserialize)]
pub struct DisclosedInput {
    pub case_id: Uuid,
    pub item_type: String,
    pub description: String,
    pub disclosed_date: Option<NaiveDate>,
    pub disclosed_by: Option<Uuid>,
    pub source_url: Option<String>,
    pub raw_metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExpectedItem {
    pub item_id: Uuid,
    pub case_id: Uuid,
    pub item_type: String,
    pub description: String,
    pub source_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DisclosedItem {
    pub item_id: Uuid,
    pub case_id: Uuid,
    pub item_type: String,
    pub description: String,
    pub disclosed_date: Option<NaiveDate>,
    pub disclosed_by: Option<Uuid>,
    pub source_url: Option<String>,
    pub raw_metadata: Value,
}

pub fn validate_type(t: &str) -> Result<(), Error> {
    if VALID_TYPES.contains(&t) {
        Ok(())
    } else {
        Err(Error::InvalidType(t.to_string()))
    }
}

pub async fn record_disclosed(
    pool: &PgPool,
    ledger: &Ledger,
    input: &DisclosedInput,
) -> Result<Uuid, Error> {
    validate_type(&input.item_type)?;
    let id = Uuid::new_v4();
    let meta = input.raw_metadata.clone().unwrap_or_else(|| json!({}));
    sqlx::query(
        "INSERT INTO disclosed_evidence_items
         (item_id, case_id, item_type, description, disclosed_date,
          disclosed_by, source_url, raw_metadata)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(input.case_id)
    .bind(&input.item_type)
    .bind(&input.description)
    .bind(input.disclosed_date)
    .bind(input.disclosed_by)
    .bind(&input.source_url)
    .bind(&meta)
    .execute(pool)
    .await?;

    ledger
        .append(
            EVENT_DISCLOSED,
            &json!({
                "item_id": id,
                "case_id": input.case_id,
                "item_type": input.item_type,
            }),
        )
        .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_validation() {
        assert!(validate_type("bodycam").is_ok());
        assert!(validate_type("secret_tape").is_err());
    }
}
