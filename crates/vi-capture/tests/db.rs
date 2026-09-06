//! Integration test against a live Postgres. Skipped unless DATABASE_URL is
//! set. Applies workspace migrations (incl. 0017_capture.sql) via vi-db and
//! exercises rebuild_edges -> compute_metrics -> outliers end to end.
#![forbid(unsafe_code)]

use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    vi_db::migrate(&pool).await.expect("migrations apply");
    Some(pool)
}

async fn cleanup(pool: &PgPool) {
    for q in [
        "DELETE FROM capture_edges WHERE court_id = 'zz-capture-test'",
        "DELETE FROM capture_metrics WHERE entity_key LIKE '%zz-capture-test%'",
        "DELETE FROM court_cases WHERE docket_number LIKE 'capture-test-%'",
    ] {
        sqlx::query(q).execute(pool).await.unwrap();
    }
}

async fn seed_opinion(pool: &PgPool, n: u32, judge: &str, text: &str) {
    let case_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO court_cases (case_id, docket_number, jurisdiction, source_court_id)
         VALUES ($1, $2, 'zz-capture-test', 'zz-capture-test')",
    )
    .bind(case_id)
    .bind(format!("capture-test-{n}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO court_opinions
            (opinion_id, case_id, judge, date_issued, full_text)
         VALUES ($1, $2, $3, DATE '2024-03-01', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(case_id)
    .bind(judge)
    .bind(text)
    .execute(pool)
    .await
    .unwrap();
}

const GRANTED: &str = "The motion to suppress is granted.";
const DENIED: &str = "Defendant's motion is denied. Affirmed.";

#[tokio::test]
async fn rebuild_compute_outliers_round_trip() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    cleanup(&pool).await;

    // Planted clique: one judge, eight straight relief outcomes.
    let mut n = 0u32;
    for _ in 0..8 {
        n += 1;
        seed_opinion(&pool, n, "Clique Judge", GRANTED).await;
    }
    // Six peers evenly split between relief and adverse.
    for j in 0..6 {
        for k in 0..6 {
            n += 1;
            let text = if k < 3 { GRANTED } else { DENIED };
            seed_opinion(&pool, n, &format!("Peer Judge {j}"), text).await;
        }
    }
    // Judge with no outcome-bearing text: no edges, no fabrication.
    n += 1;
    seed_opinion(&pool, n, "Silent Judge", "A scheduling order was entered.").await;
    // Opinion with no author: counted as no_author, never an edge.
    n += 1;
    let case_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO court_cases (case_id, docket_number, jurisdiction, source_court_id)
         VALUES ($1, $2, 'zz-capture-test', 'zz-capture-test')",
    )
    .bind(case_id)
    .bind(format!("capture-test-{n}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO court_opinions (opinion_id, case_id, date_issued, full_text)
         VALUES ($1, $2, DATE '2024-03-02', $3)",
    )
    .bind(Uuid::new_v4())
    .bind(case_id)
    .bind(GRANTED)
    .execute(&pool)
    .await
    .unwrap();

    let rebuild = vi_capture::rebuild_edges(&pool).await.unwrap();
    let our_edges: (i64,) =
        sqlx::query_as("SELECT count(*) FROM capture_edges WHERE court_id = 'zz-capture-test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(our_edges.0, 44, "{rebuild:?}");
    assert!(rebuild.no_lexicon_hit >= 1, "{rebuild:?}");
    assert!(rebuild.no_author >= 1, "{rebuild:?}");

    // Too few permutations is a hard error, not a silent degradation.
    assert!(vi_capture::compute_metrics(&pool, 10, 42).await.is_err());

    let report = vi_capture::compute_metrics(&pool, 1000, 42).await.unwrap();
    // Metrics are corpus-wide and tests share the database, so totals are
    // floors; this fixture's own entities are checked by key below.
    assert!(report.judges >= 7, "{report:?}");
    // The fixture carries no office data; a shared test database may, so the
    // office count is not asserted here.
    assert!(report.flagged >= 1, "the clique should flag: {report:?}");

    let found = vi_capture::outliers(&pool, 0.05).await.unwrap();
    let clique = found
        .iter()
        .find(|o| o.entity_key == "Clique Judge @ zz-capture-test")
        .expect("clique judge is an outlier");
    assert_eq!(clique.entity_kind, "judge");
    assert_eq!(clique.appearances, 8);
    assert_eq!(clique.status, "pending");
    assert!(clique.gini > clique.null_mean, "{clique:?}");

    // Determinism end to end: same seed, same p-values.
    vi_capture::compute_metrics(&pool, 1000, 42).await.unwrap();
    let again = vi_capture::outliers(&pool, 0.05).await.unwrap();
    let clique2 = again
        .iter()
        .find(|o| o.entity_key == "Clique Judge @ zz-capture-test" && o.status == "pending")
        .expect("clique judge still an outlier");
    assert_eq!(clique.null_p.to_bits(), clique2.null_p.to_bits());

    cleanup(&pool).await;
}
