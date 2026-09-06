//! Integration test against a live Postgres. Skipped unless DATABASE_URL is
//! set (mirrors crates/vi-api/tests). Applies the workspace migrations —
//! including 0016_drift.sql — via vi-db, then exercises the full engine path.
#![forbid(unsafe_code)]

use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    vi_db::migrate(&pool).await.expect("migrations apply");
    Some(pool)
}

/// Signal ingestion is exercised in its own court so its rows never pollute
/// the planted detection series in `PLANT_COURT`.
const INGEST_COURT: &str = "zz-drift-ingest";
const PLANT_COURT: &str = "zz-drift-test";
const CLAUSE: &str = "amend.04.search";

async fn cleanup(pool: &PgPool) {
    for q in [
        "DELETE FROM drift_changepoints WHERE court_id IN ('zz-drift-ingest','zz-drift-test')",
        "DELETE FROM drift_runs WHERE court_id IN ('zz-drift-ingest','zz-drift-test')",
        "DELETE FROM drift_observations WHERE court_id IN ('zz-drift-ingest','zz-drift-test')",
        "DELETE FROM constitution_screen_hits WHERE screen_id IN (SELECT screen_id FROM constitution_screens WHERE jurisdiction IN ('zz-drift-ingest','zz-drift-test'))",
        "DELETE FROM constitution_screens WHERE jurisdiction IN ('zz-drift-ingest','zz-drift-test')",
        "DELETE FROM court_cases WHERE docket_number LIKE 'drift-test-%'",
    ] {
        sqlx::query(q).execute(pool).await.unwrap();
    }
}

/// One case + opinion + constitution screen with a clause hit, so
/// ingest_signals has something to join against.
async fn seed_opinion(pool: &PgPool, court: &str, n: u32, text: &str, source_ref: &str) {
    let case_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO court_cases (case_id, docket_number, jurisdiction, source_court_id)
         VALUES ($1, $2, $3, $3)",
    )
    .bind(case_id)
    .bind(format!("drift-test-{n}"))
    .bind(court)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO court_opinions
            (opinion_id, case_id, judge, date_issued, full_text, source_ref)
         VALUES ($1, $2, 'Drift T.', DATE '2024-02-01', $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(case_id)
    .bind(text)
    .bind(source_ref)
    .execute(pool)
    .await
    .unwrap();
    let screen_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO constitution_screens
            (screen_id, case_id, jurisdiction, snapshot_id, corpus_hash, hit_count, report)
         VALUES ($1, $2, $3, 'test-snapshot', 'test-hash', 1, '{}')",
    )
    .bind(screen_id)
    .bind(case_id)
    .bind(court)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO constitution_screen_hits
            (screen_id, clause_id, authority, citation, severity, matched, resolution)
         VALUES ($1, 'amend.04.search', 'controlling', NULL, 'medium', ARRAY['test'], '{}')",
    )
    .bind(screen_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn ingest_signals_labels_and_is_idempotent() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    cleanup(&pool).await;

    seed_opinion(&pool, INGEST_COURT, 1, "The motion to suppress is granted.", "drift-test:grant").await;
    seed_opinion(&pool, INGEST_COURT, 2, "The parties stipulated to a scheduling order.", "drift-test:none").await;

    let first = vi_drift::ingest_signals(&pool).await.unwrap();
    // The ingest is corpus-wide; other tests share this database, so counts
    // beyond this test's own rows are asserted as floors, not equalities.
    assert!(first.inserted >= 1, "{first:?}");
    let second = vi_drift::ingest_signals(&pool).await.unwrap();
    assert_eq!(second.inserted, 0, "re-ingest must be idempotent: {second:?}");
    assert!(second.already_present >= 1, "{second:?}");
    assert!(second.no_lexicon_hit >= 1, "silent text makes no row: {second:?}");

    let stored: (bool, f64) = sqlx::query_as(
        "SELECT machine_derived, signal FROM drift_observations
         WHERE court_id = 'zz-drift-ingest' AND source_ref = 'drift-test:grant'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(stored.0, "every ingested signal is machine_derived");
    assert!(stored.1 > 0.95, "granted text scores near 1: {stored:?}");

    cleanup(&pool).await;
}

#[tokio::test]
async fn detect_finds_planted_shift_and_ignores_stationary() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    cleanup(&pool).await;

    // Plant a regime shift: six adverse signals, then six relief signals.
    for i in 0..12 {
        let signal = if i < 6 { 0.05 } else { 0.95 };
        sqlx::query(
            "INSERT INTO drift_observations
                (court_id, clause_id, observed_at, signal, source_ref)
             VALUES ('zz-drift-test', 'amend.04.search',
                     DATE '2024-01-01' + ($1 || ' days')::interval, $2, $3)",
        )
        .bind(i * 10)
        .bind(signal)
        .bind(format!("drift-test:planted-{i}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    let found = vi_drift::detect(&pool, PLANT_COURT, CLAUSE, 20.0).await.unwrap();
    assert_eq!(found.len(), 1, "one planted shift, one changepoint: {found:?}");
    let cp = &found[0];
    assert_eq!(cp.status, "pending");
    assert!(cp.posterior >= 0.5, "posterior {}", cp.posterior);
    // Planted between index 5 and 6; allow the detector a few days of slack.
    let lo = chrono::NaiveDate::from_ymd_opt(2024, 2, 20).unwrap();
    let hi = chrono::NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
    assert!(
        cp.at_date >= lo && cp.at_date <= hi,
        "changepoint at {} should sit on the plant",
        cp.at_date
    );
    assert!(
        cp.window.to_string().contains("\"run_id\""),
        "window carries context"
    );

    let listed = vi_drift::list_changepoints(&pool, 0.5).await.unwrap();
    assert!(listed.iter().any(|c| c.id == cp.id));
    let filtered = vi_drift::list_changepoints(&pool, cp.posterior + 0.01)
        .await
        .unwrap();
    assert!(!filtered.iter().any(|c| c.id == cp.id));

    // A stationary series must not alarm.
    for i in 0..12 {
        sqlx::query(
            "INSERT INTO drift_observations
                (court_id, clause_id, observed_at, signal, source_ref)
             VALUES ('zz-drift-test', 'amend.08.bail',
                     DATE '2024-01-01' + ($1 || ' days')::interval, 0.5, $2)",
        )
        .bind(i * 10)
        .bind(format!("drift-test:flat-{i}"))
        .execute(&pool)
        .await
        .unwrap();
    }
    let none = vi_drift::detect(&pool, PLANT_COURT, "amend.08.bail", 20.0)
        .await
        .unwrap();
    assert!(none.is_empty(), "stationary series alarmed: {none:?}");

    // Invalid hazard is a hard error.
    assert!(vi_drift::detect(&pool, PLANT_COURT, CLAUSE, 0.0).await.is_err());

    cleanup(&pool).await;
}
