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

/// Terms that evidence a signal in public text. Matching is literal and
/// case-insensitive: an occurrence records that the record *mentions* the
/// doctrine, never that a named person used the tactic. Occurrences are
/// leads and stay pending until counsel review.
pub fn signal_terms(signal: &str) -> &'static [&'static str] {
    match signal {
        "brady" => &["brady"],
        "giglio" => &["giglio"],
        "batson" => &["batson"],
        "discovery" => &["discovery violation", "late discovery", "sandbagging"],
        "informant" => &["informant", "cooperating witness"],
        "trial_penalty" => &["trial penalty", "rejected a plea", "rejected the plea"],
        // Charge stacking is a structured signal (cardinality of charges),
        // not a text mention; matched separately.
        "charge_stack" => &[],
        _ => &[],
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Occurrence {
    pub occurrence_id: Uuid,
    pub tactic_id: Uuid,
    pub signal: Option<String>,
    pub matched_term: String,
    pub match_source: &'static str,
}

/// Match one case's public record against the tactic catalog and record each
/// hit as a pending occurrence. Idempotent per (case, tactic, term, source).
///
/// Returns the number of *new* occurrences inserted. A case with no opinion
/// text and no charges has nothing to match against; the caller reports that
/// as skipped, and this function is not invoked.
pub async fn match_case_occurrences(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
) -> Result<Vec<Occurrence>, Error> {
    let texts: Vec<String> = sqlx::query_scalar(
        "SELECT full_text FROM court_opinions WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await?;
    let haystack = texts.join("\n").to_lowercase();

    let n_charges: i64 = sqlx::query_scalar(
        "SELECT COALESCE(cardinality(charges), 0)::bigint FROM court_cases WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await?;

    let catalog = list(pool, None).await?;
    let mut inserted = Vec::new();
    for tactic in &catalog {
        let Some(signal) = tactic.signal.as_deref() else {
            continue;
        };
        let mut matches: Vec<(&str, &'static str)> = signal_terms(signal)
            .iter()
            .filter(|term| haystack.contains(**term))
            .map(|term| (*term, "opinion_text"))
            .collect();
        if signal == "charge_stack" && n_charges > 1 {
            matches.push(("cardinality(charges) > 1", "charges"));
        }
        for (term, source) in matches {
            let row = sqlx::query_as::<_, (Uuid,)>(
                "INSERT INTO tactic_occurrences (case_id, tactic_id, matched_term, match_source)
                 VALUES ($1,$2,$3,$4)
                 ON CONFLICT (case_id, tactic_id, matched_term, match_source) DO NOTHING
                 RETURNING occurrence_id",
            )
            .bind(case_id)
            .bind(tactic.tactic_id)
            .bind(term)
            .bind(source)
            .fetch_optional(pool)
            .await?;
            if let Some((occurrence_id,)) = row {
                inserted.push(Occurrence {
                    occurrence_id,
                    tactic_id: tactic.tactic_id,
                    signal: tactic.signal.clone(),
                    matched_term: term.to_string(),
                    match_source: source,
                });
            }
        }
    }

    if !inserted.is_empty() {
        ledger
            .append(
                vi_ledger::events::TACTIC_OCCURRENCE,
                &json!({
                    "case_id": case_id,
                    "occurrences": inserted.len(),
                    "occurrence_ids": inserted.iter().map(|o| o.occurrence_id).collect::<Vec<_>>(),
                    "review_status": "pending",
                    "note": "A mention of a doctrine in public text. An occurrence \
                             accuses no one and publishes nothing until counsel review.",
                }),
            )
            .await?;
    }
    Ok(inserted)
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

    #[test]
    fn every_catalog_signal_has_a_match_rule() {
        for signal in VALID_SIGNALS {
            if *signal == "charge_stack" {
                assert!(signal_terms(signal).is_empty());
            } else {
                assert!(!signal_terms(signal).is_empty(), "{signal} has no terms");
            }
        }
        assert!(signal_terms("unknown-signal").is_empty());
    }
}
