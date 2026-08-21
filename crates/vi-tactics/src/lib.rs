//! Tactics DB: curated public-record catalog of prosecution / defense /
//! judicial tactics, with rates recomputed from substantiated findings and
//! docket outcomes. Rates are *observed occurrence among public records*,
//! not a claim that a named person "uses" a tactic.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

pub const EVENT: &str = vi_ledger::events::TACTIC_RECORDED;

pub const VALID_CATEGORIES: &[&str] = &["prosecution", "defense", "judicial"];
pub const VALID_SIGNALS: &[&str] = &[
    "brady",
    "giglio",
    "batson",
    "discovery",
    "informant",
    "trial_penalty",
    "charge_stack",
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("invalid category: {0}")]
    InvalidCategory(String),
    #[error("invalid signal: {0}")]
    InvalidSignal(String),
    #[error("tactic not found")]
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tactic {
    pub tactic_id: Uuid,
    pub description: String,
    pub category: String,
    pub signal: Option<String>,
    pub success_rate: Option<f32>,
    pub data_points: i32,
    pub source_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewTactic {
    pub description: String,
    pub category: String,
    pub signal: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TacticStats {
    pub tactic_id: Uuid,
    pub signal: Option<String>,
    pub data_points: i64,
    pub hits: i64,
    pub rate: Option<f64>,
    pub formula: &'static str,
}

pub fn validate_category(c: &str) -> Result<(), Error> {
    if VALID_CATEGORIES.contains(&c) {
        Ok(())
    } else {
        Err(Error::InvalidCategory(c.to_string()))
    }
}

pub fn validate_signal(s: &str) -> Result<(), Error> {
    if VALID_SIGNALS.contains(&s) {
        Ok(())
    } else {
        Err(Error::InvalidSignal(s.to_string()))
    }
}

pub async fn list(pool: &PgPool, category: Option<&str>) -> Result<Vec<Tactic>, Error> {
    let rows = sqlx::query_as::<_, Tactic>(
        "SELECT tactic_id, description, category, signal, success_rate, data_points, source_url
         FROM tactics
         WHERE ($1::text IS NULL OR category = $1)
         ORDER BY category, description",
    )
    .bind(category)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Tactic, Error> {
    sqlx::query_as::<_, Tactic>(
        "SELECT tactic_id, description, category, signal, success_rate, data_points, source_url
         FROM tactics WHERE tactic_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound)
}

pub async fn create(pool: &PgPool, ledger: &Ledger, input: &NewTactic) -> Result<Uuid, Error> {
    validate_category(&input.category)?;
    if let Some(s) = input.signal.as_deref() {
        validate_signal(s)?;
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tactics (tactic_id, description, category, signal, source_url)
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(&input.description)
    .bind(&input.category)
    .bind(&input.signal)
    .bind(&input.source_url)
    .execute(pool)
    .await?;

    ledger
        .append(
            EVENT,
            &json!({
                "tactic_id": id,
                "description": input.description,
                "category": input.category,
                "signal": input.signal,
            }),
        )
        .await?;
    Ok(id)
}

/// Recompute occurrence rate from public-record tables and persist it.
pub async fn refresh_stats(pool: &PgPool, id: Uuid) -> Result<TacticStats, Error> {
    let tactic = get(pool, id).await?;
    let stats = compute_stats(pool, &tactic).await?;
    sqlx::query("UPDATE tactics SET data_points = $1, success_rate = $2 WHERE tactic_id = $3")
        .bind(stats.data_points as i32)
        .bind(stats.rate.map(|r| r as f32))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(stats)
}

pub async fn compute_stats(pool: &PgPool, tactic: &Tactic) -> Result<TacticStats, Error> {
    let signal = tactic.signal.as_deref().unwrap_or("");
    let (data_points, hits) = match signal {
        "brady" | "giglio" | "batson" | "discovery" => {
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM court_cases")
                .fetch_one(pool)
                .await?;
            let h: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM constitutional_findings
                 WHERE finding_type = $1 AND review_status = 'substantiated'",
            )
            .bind(signal)
            .fetch_one(pool)
            .await?;
            (n, h)
        }
        "informant" => {
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM court_cases")
                .fetch_one(pool)
                .await?;
            let h: i64 = sqlx::query_scalar(
                "SELECT COUNT(DISTINCT case_id) FROM expected_evidence_items
                 WHERE item_type = 'informant_benefit'",
            )
            .fetch_one(pool)
            .await?;
            (n, h)
        }
        "trial_penalty" => {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM court_cases
                 WHERE plea_offered AND plea_offer_months > 0",
            )
            .fetch_one(pool)
            .await?;
            let h: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM court_cases
                 WHERE plea_offered AND NOT plea_accepted AND outcome = 'conviction'
                   AND plea_offer_months > 0 AND sentence_months > 0
                   AND sentence_months::float / plea_offer_months >= 2.0",
            )
            .fetch_one(pool)
            .await?;
            (n, h)
        }
        "charge_stack" => {
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM court_cases")
                .fetch_one(pool)
                .await?;
            let h: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM court_cases WHERE cardinality(charges) > 1",
            )
            .fetch_one(pool)
            .await?;
            (n, h)
        }
        _ => (0, 0),
    };

    let rate = if data_points > 0 {
        Some(hits as f64 / data_points as f64)
    } else {
        None
    };

    Ok(TacticStats {
        tactic_id: tactic.tactic_id,
        signal: tactic.signal.clone(),
        data_points,
        hits,
        rate,
        formula: "rate = hits / data_points; hits are substantiated findings or docket signals matching tactic.signal (public records only)",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_and_signal_validation() {
        assert!(validate_category("prosecution").is_ok());
        assert!(validate_category("espionage").is_err());
        assert!(validate_signal("brady").is_ok());
        assert!(validate_signal("osint").is_err());
    }
}
