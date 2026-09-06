//! Database boundary: edge rebuild, metric computation, outlier reads.
//! All concentration math lives in [`crate::stats`]; the outcome proxy comes
//! from [`vi_drift::lexicon`] so both engines read opinions the same way.
//! Every artifact is machine-derived and `pending` — a low p-value is a lead
//! for counsel, never a finding of capture.
#![forbid(unsafe_code)]

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::stats;

/// Ledger event types appended by this engine.
pub const EVENT_EDGES_REBUILT: &str = "CaptureEdgesRebuilt";
pub const EVENT_METRICS_COMPUTED: &str = "CaptureMetricsComputed";

/// Entities with fewer appearances cannot support a concentration claim.
pub const MIN_APPEARANCES: usize = 3;

/// The null model is meaningless below this many permutations.
pub const MIN_PERMUTATIONS: u32 = 1000;

/// Flag threshold used for the report's `flagged` count.
pub const FLAG_MAX_P: f64 = 0.05;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("permutations must be >= {MIN_PERMUTATIONS}, got {0}")]
    TooFewPermutations(u32),
}

#[derive(Debug, Clone, Serialize)]
pub struct RebuildReport {
    pub opinions_scanned: u64,
    pub edges: u64,
    pub no_author: u64,
    pub no_lexicon_hit: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureReport {
    pub entities: usize,
    pub judges: usize,
    pub offices: usize,
    pub flagged: usize,
    pub skipped_min_appearances: usize,
    pub permutations: u32,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricOutlier {
    pub id: Uuid,
    pub entity_kind: String,
    pub entity_key: String,
    pub appearances: i32,
    pub gini: f64,
    pub entropy: f64,
    pub null_mean: f64,
    pub null_p: f64,
    pub status: String,
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Rebuild the edge table from ingested opinions. An edge exists only where
/// the opinion names an authoring judge (court_opinions.judge, populated from
/// CourtListener's author_str) AND the lexicon read an outcome proxy off the
/// text. The table is machine-derived scratch state, so a rebuild replaces it
/// wholesale inside one transaction.
pub async fn rebuild_edges(pool: &PgPool) -> Result<RebuildReport, Error> {
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<NaiveDate>,
            chrono::DateTime<chrono::Utc>,
            Option<Uuid>,
        ),
    >(
        "SELECT o.opinion_id,
                NULLIF(btrim(o.judge), '')           AS judge_name,
                COALESCE(c.source_court_id, c.jurisdiction) AS court_id,
                p.office,
                o.full_text,
                o.date_issued,
                o.ingested_at,
                o.case_id
         FROM court_opinions o
         JOIN court_cases c ON c.case_id = o.case_id
         LEFT JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id",
    )
    .fetch_all(pool)
    .await?;

    let mut report = RebuildReport {
        opinions_scanned: rows.len() as u64,
        edges: 0,
        no_author: 0,
        no_lexicon_hit: 0,
    };

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM capture_edges")
        .execute(&mut *tx)
        .await?;

    for (opinion_id, judge, court_id, office, text, date_issued, ingested_at, case_id) in rows {
        let Some(judge_name) = judge else {
            report.no_author += 1;
            continue;
        };
        let Some(signal) = vi_drift::lexicon::outcome_signal(&text) else {
            report.no_lexicon_hit += 1;
            continue;
        };
        let court_id = court_id.unwrap_or_else(|| "unknown".to_string());
        let observed_at = date_issued.unwrap_or_else(|| ingested_at.date_naive());
        sqlx::query(
            "INSERT INTO capture_edges
                (judge_name, court_id, office, outcome_signal, case_id, observed_at,
                 source_ref, machine_derived)
             VALUES ($1,$2,$3,$4,$5,$6,$7,TRUE)
             ON CONFLICT (judge_name, court_id, source_ref) DO NOTHING",
        )
        .bind(&judge_name)
        .bind(&court_id)
        .bind(&office)
        .bind(signal)
        .bind(case_id)
        .bind(observed_at)
        .bind(format!("opinion:{opinion_id}"))
        .execute(&mut *tx)
        .await?;
        report.edges += 1;
    }
    tx.commit().await?;

    vi_ledger::Ledger::new(pool.clone())
        .append(
            EVENT_EDGES_REBUILT,
            &serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
        )
        .await?;

    Ok(report)
}

/// Compute per-entity concentration metrics against a seeded, degree-
/// preserving Monte Carlo null and store them as `pending` rows.
///
/// Two populations are scored independently: judges (`entity_key` =
/// "Name @ court_id") and offices (edges that carry one). `permutations`
/// must be >= [`MIN_PERMUTATIONS`]; `seed` fully determines the null.
pub async fn compute_metrics(
    pool: &PgPool,
    permutations: u32,
    seed: u64,
) -> Result<CaptureReport, Error> {
    if permutations < MIN_PERMUTATIONS {
        return Err(Error::TooFewPermutations(permutations));
    }

    let edges = sqlx::query_as::<_, (String, String, Option<String>, f64)>(
        "SELECT judge_name, court_id, office, outcome_signal FROM capture_edges",
    )
    .fetch_all(pool)
    .await?;

    let mut judges: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut offices: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (judge, court, office, signal) in &edges {
        judges
            .entry(format!("{judge} @ {court}"))
            .or_default()
            .push(stats::outcome_category(*signal));
        if let Some(office) = office {
            offices
                .entry(office.clone())
                .or_default()
                .push(stats::outcome_category(*signal));
        }
    }

    let to_entities = |map: BTreeMap<String, Vec<usize>>| -> (Vec<stats::EntityObservations>, usize) {
        let mut skipped = 0usize;
        let entities = map
            .into_iter()
            .filter_map(|(key, categories)| {
                if categories.len() >= MIN_APPEARANCES {
                    Some(stats::EntityObservations { key, categories })
                } else {
                    skipped += 1;
                    None
                }
            })
            .collect();
        (entities, skipped)
    };
    let (judge_entities, skipped_j) = to_entities(judges);
    let (office_entities, skipped_o) = to_entities(offices);

    // One caller-visible seed drives both populations; the office stream is a
    // fixed derivation of it so a run stays fully reproducible from `seed`.
    let office_seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let judge_outcomes = stats::monte_carlo_null(&judge_entities, permutations as usize, seed);
    let office_outcomes =
        stats::monte_carlo_null(&office_entities, permutations as usize, office_seed);

    // Pending rows are machine output: replaced on recompute. Rows counsel
    // has reviewed (substantiated/rejected) are decisions and stay put.
    sqlx::query("DELETE FROM capture_metrics WHERE status = 'pending'")
        .execute(pool)
        .await?;

    let mut flagged = 0usize;
    for (kind, outcomes) in [("judge", &judge_outcomes), ("office", &office_outcomes)] {
        for o in outcomes {
            if o.null_p <= FLAG_MAX_P {
                flagged += 1;
            }
            sqlx::query(
                "INSERT INTO capture_metrics
                    (entity_kind, entity_key, appearances, gini, entropy,
                     null_mean, null_p, status)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,'pending')",
            )
            .bind(kind)
            .bind(&o.key)
            .bind(o.appearances as i32)
            .bind(o.observed_gini)
            .bind(o.observed_entropy)
            .bind(o.null_mean_gini)
            .bind(o.null_p)
            .execute(pool)
            .await?;
        }
    }

    let report = CaptureReport {
        entities: judge_outcomes.len() + office_outcomes.len(),
        judges: judge_outcomes.len(),
        offices: office_outcomes.len(),
        flagged,
        skipped_min_appearances: skipped_j + skipped_o,
        permutations,
        seed,
    };

    vi_ledger::Ledger::new(pool.clone())
        .append(
            EVENT_METRICS_COMPUTED,
            &serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
        )
        .await?;

    Ok(report)
}

/// Stored metrics at or below `max_p`, most significant first.
pub async fn outliers(pool: &PgPool, max_p: f64) -> Result<Vec<MetricOutlier>, Error> {
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            i32,
            f64,
            f64,
            f64,
            f64,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT id, entity_kind, entity_key, appearances, gini, entropy,
                null_mean, null_p, status, computed_at
         FROM capture_metrics
         WHERE null_p <= $1
         ORDER BY null_p ASC, appearances DESC",
    )
    .bind(max_p)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, entity_kind, entity_key, appearances, gini, entropy, null_mean, null_p, status, computed_at)| {
                MetricOutlier {
                    id,
                    entity_kind,
                    entity_key,
                    appearances,
                    gini,
                    entropy,
                    null_mean,
                    null_p,
                    status,
                    computed_at,
                }
            },
        )
        .collect())
}
