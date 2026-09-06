//! Database boundary: signal ingestion, detection runs, changepoint reads.
//! All analysis math lives in [`crate::bocpd`] and [`crate::lexicon`]; this
//! module only moves rows. Every emitted artifact is machine-derived and
//! `pending` — nothing here publishes a finding.
#![forbid(unsafe_code)]

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{bocpd, lexicon};

/// Ledger event types appended by this engine (vi-ledger constants are frozen
/// by another crate; these strings are the engine's own contract).
pub const EVENT_SIGNALS_INGESTED: &str = "DriftSignalsIngested";
pub const EVENT_RUN_COMPUTED: &str = "DriftRunComputed";

/// Fewer observations than this cannot support a changepoint claim; the run
/// is still recorded (with zero changepoints) so the gap is visible.
pub const MIN_OBSERVATIONS: usize = 8;

/// Reset-posterior threshold for declaring a changepoint.
pub const DETECTION_THRESHOLD: f64 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("invalid detector parameter: {0}")]
    InvalidParameter(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    pub rows_scanned: u64,
    pub inserted: u64,
    pub already_present: u64,
    pub no_lexicon_hit: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Changepoint {
    pub id: Uuid,
    pub court_id: String,
    pub clause_id: String,
    pub at_date: NaiveDate,
    pub posterior: f64,
    pub window: serde_json::Value,
    pub status: String,
}

type ObservationRow = (String, String, NaiveDate, f64, String);

/// Build per-(court, clause) outcome-signal observations from ingested
/// opinions joined to their constitution-screen clause hits.
///
/// Idempotent: (court_id, clause_id, source_ref) is unique, so re-running
/// after a re-ingest adds only new opinions. Rows only exist where the
/// lexicon fired — a silent text is a gap, not a zero.
pub async fn ingest_signals(pool: &PgPool) -> Result<IngestReport, Error> {
    // DISTINCT ON: a case screened more than once must not double-count the
    // same opinion/clause pair.
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Option<String>,
            String,
            Option<NaiveDate>,
            chrono::DateTime<chrono::Utc>,
            String,
            String,
        ),
    >(
        "SELECT DISTINCT ON (o.opinion_id, h.clause_id)
                o.opinion_id, o.source_ref, o.full_text, o.date_issued, o.ingested_at,
                COALESCE(c.source_court_id, c.jurisdiction) AS court_id,
                h.clause_id
         FROM court_opinions o
         JOIN court_cases c ON c.case_id = o.case_id
         JOIN constitution_screens s ON s.case_id = o.case_id
         JOIN constitution_screen_hits h ON h.screen_id = s.screen_id
         ORDER BY o.opinion_id, h.clause_id, s.created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    let mut report = IngestReport {
        rows_scanned: rows.len() as u64,
        inserted: 0,
        already_present: 0,
        no_lexicon_hit: 0,
    };

    for (opinion_id, source_ref, text, date_issued, ingested_at, court_id, clause_id) in rows {
        let Some(signal) = lexicon::outcome_signal(&text) else {
            report.no_lexicon_hit += 1;
            continue;
        };
        let source_ref = source_ref.unwrap_or_else(|| format!("opinion:{opinion_id}"));
        let observed_at = date_issued.unwrap_or_else(|| ingested_at.date_naive());
        let inserted = sqlx::query(
            "INSERT INTO drift_observations
                (court_id, clause_id, observed_at, signal, source_ref, machine_derived)
             VALUES ($1,$2,$3,$4,$5,TRUE)
             ON CONFLICT (court_id, clause_id, source_ref) DO NOTHING",
        )
        .bind(&court_id)
        .bind(&clause_id)
        .bind(observed_at)
        .bind(signal)
        .bind(&source_ref)
        .execute(pool)
        .await?
        .rows_affected()
            > 0;
        if inserted {
            report.inserted += 1;
        } else {
            report.already_present += 1;
        }
    }

    vi_ledger::Ledger::new(pool.clone())
        .append(EVENT_SIGNALS_INGESTED, &serde_json::to_value(&report).unwrap_or_else(|_| json!({})))
        .await?;

    Ok(report)
}

/// Run BOCPD over the (court, clause) series and record what it finds.
///
/// `hazard` is the expected segment length λ of the constant-hazard model
/// (H = 1/λ); larger values make the detector more conservative. Every call
/// appends a `drift_runs` row; previously recorded `pending` changepoints for
/// the pair are replaced by the fresh computation (substantiated/rejected
/// rows are counsel decisions and are never touched).
pub async fn detect(
    pool: &PgPool,
    court_id: &str,
    clause_id: &str,
    hazard: f64,
) -> Result<Vec<Changepoint>, Error> {
    if !(hazard >= 2.0 && hazard.is_finite()) {
        return Err(Error::InvalidParameter(format!(
            "hazard (expected run length) must be finite and >= 2, got {hazard}"
        )));
    }

    let observations = sqlx::query_as::<_, ObservationRow>(
        "SELECT court_id, clause_id, observed_at, signal, source_ref
         FROM drift_observations
         WHERE court_id = $1 AND clause_id = $2
         ORDER BY observed_at, id",
    )
    .bind(court_id)
    .bind(clause_id)
    .fetch_all(pool)
    .await?;

    let series: Vec<f64> = observations.iter().map(|r| r.3).collect();
    let detected = if series.len() >= MIN_OBSERVATIONS {
        bocpd::detect_changepoints(&series, hazard, DETECTION_THRESHOLD)
            .map_err(Error::InvalidParameter)?
    } else {
        Vec::new()
    };

    // Record the run itself, including the final run-length posterior
    // (truncated to the most likely runs) so the detector state is auditable.
    let mut model = bocpd::Bocpd::new(bocpd::Prior::default(), hazard)
        .map_err(Error::InvalidParameter)?;
    for &x in &series {
        model.step(x);
    }
    let final_posterior: Vec<serde_json::Value> = model
        .run_length_posterior()
        .iter()
        .enumerate()
        .filter(|(_, p)| **p >= 1e-4)
        .take(32)
        .map(|(r, p)| json!({"run_length": r, "p": p}))
        .collect();
    let run_id = Uuid::new_v4();
    let summary = json!({
        "hazard_expected_run_length": hazard,
        "detection_threshold": DETECTION_THRESHOLD,
        "observations": series.len(),
        "min_observations": MIN_OBSERVATIONS,
        "insufficient_data": series.len() < MIN_OBSERVATIONS,
        "changepoints": detected.len(),
        "final_run_length_posterior": final_posterior,
    });
    sqlx::query(
        "INSERT INTO drift_runs (id, court_id, clause_id, run_length)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(run_id)
    .bind(court_id)
    .bind(clause_id)
    .bind(&summary)
    .execute(pool)
    .await?;

    // Recompute, never duplicate: pending rows for this pair are machine
    // output; reviewed rows belong to counsel and stay untouched.
    sqlx::query(
        "DELETE FROM drift_changepoints
         WHERE court_id = $1 AND clause_id = $2 AND status = 'pending'",
    )
    .bind(court_id)
    .bind(clause_id)
    .execute(pool)
    .await?;

    let mut out = Vec::new();
    for cp in &detected {
        let obs = &observations[cp.index];
        let window = json!({
            "run_id": run_id,
            "index": cp.index,
            "n_observations": series.len(),
            "window": observations
                .iter()
                .enumerate()
                .filter(|(i, _)| (*i + 3 >= cp.index) && (*i <= cp.index + 3))
                .map(|(i, r)| json!({
                    "index": i,
                    "observed_at": r.2,
                    "signal": r.3,
                    "source_ref": r.4,
                }))
                .collect::<Vec<_>>(),
        });
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO drift_changepoints
                (id, court_id, clause_id, at_date, posterior, "window", status)
             VALUES ($1,$2,$3,$4,$5,$6,'pending')"#,
        )
        .bind(id)
        .bind(court_id)
        .bind(clause_id)
        .bind(obs.2)
        .bind(cp.posterior)
        .bind(&window)
        .execute(pool)
        .await?;
        out.push(Changepoint {
            id,
            court_id: court_id.to_string(),
            clause_id: clause_id.to_string(),
            at_date: obs.2,
            posterior: cp.posterior,
            window,
            status: "pending".to_string(),
        });
    }

    vi_ledger::Ledger::new(pool.clone())
        .append(
            EVENT_RUN_COMPUTED,
            &json!({
                "run_id": run_id,
                "court_id": court_id,
                "clause_id": clause_id,
                "observations": series.len(),
                "changepoints": detected.len(),
            }),
        )
        .await?;

    Ok(out)
}

/// All recorded changepoints at or above `min_posterior`, strongest first.
pub async fn list_changepoints(
    pool: &PgPool,
    min_posterior: f64,
) -> Result<Vec<Changepoint>, Error> {
    let rows = sqlx::query_as::<
        _,
        (Uuid, String, String, NaiveDate, f64, serde_json::Value, String),
    >(
        r#"SELECT id, court_id, clause_id, at_date, posterior, "window", status
         FROM drift_changepoints
         WHERE posterior >= $1
         ORDER BY posterior DESC, at_date DESC"#,
    )
    .bind(min_posterior)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, court_id, clause_id, at_date, posterior, window, status)| Changepoint {
            id,
            court_id,
            clause_id,
            at_date,
            posterior,
            window,
            status,
        })
        .collect())
}
