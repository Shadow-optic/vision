//! Shared pool + the case-context builder that feeds TrustScript.
//! Derived features are computed HERE — in reviewable Rust — never in rules.
#![forbid(unsafe_code)]

use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("DATABASE_URL is not set")]
    MissingDatabaseUrl,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Constitution(#[from] vi_constitution::db::Error),
}

pub async fn pool_from_env() -> Result<PgPool, Error> {
    let url = std::env::var("DATABASE_URL").map_err(|_| Error::MissingDatabaseUrl)?;
    let max = std::env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    Ok(PgPoolOptions::new()
        .max_connections(max)
        .connect(&url)
        .await?)
}

/// Applies sqlx migrations from the workspace `migrations/` directory.
/// Embedded at compile time — no DATABASE_URL needed to *build*.
pub async fn migrate(pool: &PgPool) -> Result<(), Error> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    vi_constitution::db::sync_native(pool).await?;
    Ok(())
}

pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// Builds the evaluation context: {"case": {...}} including derived fields.
/// `plea_sentence_ratio` = plea offer / actual trial sentence (only when the
/// offer was rejected and a conviction followed — i.e., the coercion window).
pub async fn case_context(pool: &PgPool, case_id: Uuid) -> Result<Option<Value>, sqlx::Error> {
    let ctx = sqlx::query_scalar::<_, Value>(
        r#"SELECT jsonb_build_object('case', jsonb_build_object(
             'case_id',            c.case_id,
             'docket_number',      c.docket_number,
             'jurisdiction',       c.jurisdiction,
             'court_level',        c.court_level,
             'charge_category',    c.charge_category,
             'defendant_race',     c.defendant_race,
             'evidence_strength',  c.evidence_strength,
             'outcome',            c.outcome,
             'plea_offered',       c.plea_offered,
             'plea_accepted',      c.plea_accepted,
             'plea_offer_months',  c.plea_offer_months,
             'sentence_months',    c.sentence_months,
             'court_h3_cell',      c.court_h3_cell,
             'judge',              c.judge,
             'plea_sentence_ratio',
               CASE WHEN c.plea_offered AND NOT c.plea_accepted
                         AND c.outcome = 'conviction'
                         AND c.plea_offer_months IS NOT NULL
                         AND c.sentence_months > 0
                    THEN c.plea_offer_months::float / c.sentence_months END,
             'prosecutor_id',      p.prosecutor_id,
             'prosecutor_name',    p.name,
             'office',             p.office
           ))
         FROM court_cases c
         LEFT JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
         WHERE c.case_id = $1"#,
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?;
    if let Some(mut v) = ctx {
        vi_constitution::attach_features(&mut v);
        Ok(Some(v))
    } else {
        Ok(None)
    }
}
