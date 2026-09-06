//! DB-backed integration tests. Skipped silently when DATABASE_URL is unset
//! or unreachable — pure unit tests (stats.rs, lib.rs) are the CI gate;
//! these run wherever a live Postgres with the workspace migrations exists.

use sqlx::PgPool;
use uuid::Uuid;

async fn try_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    PgPool::connect(&url).await.ok()
}

#[tokio::test]
async fn compute_all_on_seeded_flags() {
    let Some(pool) = try_pool().await else {
        eprintln!("DATABASE_URL unreachable; skipping DB integration test");
        return;
    };
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    // Isolated fixture: 10 fresh cases, 9 with one pending high flag,
    // 1 with three pending high flags (the resonant case).
    let mut ids = Vec::new();
    for _ in 0..10 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO court_cases (case_id, jurisdiction)
             VALUES (gen_random_uuid(), 'ZZ-TEST') RETURNING case_id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        ids.push(id);
    }
    for (i, id) in ids.iter().enumerate() {
        let n = if i == 0 { 3 } else { 1 };
        for _ in 0..n {
            sqlx::query(
                "INSERT INTO abuse_flags (case_id, label, severity)
                 VALUES ($1, 'integration-test', 'high')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    let report = vi_resonance::compute_all(&pool).await.unwrap();
    assert!(report.scored >= 10);

    let hot = vi_resonance::case_detail(&pool, ids[0])
        .await
        .unwrap()
        .expect("fixture case must be scored");
    assert_eq!(hot.n_signals, 1); // only the abuse signal has a corpus
    assert_eq!(hot.status, "pending");
    assert_eq!(hot.signals["machine_derived"], serde_json::json!(true));
    // Corpus is the 10 fixture cases; max value → p = (1+1)/(10+1).
    let cold = vi_resonance::case_detail(&pool, ids[1]).await.unwrap().unwrap();
    assert!(hot.fisher_p < cold.fisher_p);
    assert!(hot.q_value <= cold.q_value);
    // Single-signal Fisher returns the p-value itself. Exact value check only
    // when the corpus is exactly our 10 fixture cases (no other pending flags
    // in the DB): max value → p = (1+1)/(10+1).
    let corpus_n = hot.signals["items"][0]["corpus_n"].as_u64().unwrap();
    if corpus_n == 10 {
        assert!((hot.fisher_p - 2.0 / 11.0).abs() < 1e-9);
    }

    // Cleanup so reruns stay deterministic.
    for id in &ids {
        sqlx::query("DELETE FROM court_cases WHERE case_id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM case_resonance")
        .execute(&pool)
        .await
        .unwrap();
}
