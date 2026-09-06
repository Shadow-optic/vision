//! HTTP integration tests for the transparency / resonance / drift / capture
//! meta-engines. Skipped unless DATABASE_URL is set.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
use vi_api::handlers::AppState;
use vi_ledger::Ledger;

async fn router() -> Option<(axum::Router, PgPool)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()?;
    vi_db::migrate(&pool).await.ok()?;
    let app = vi_api::router(AppState {
        ledger: Ledger::new(pool.clone()),
        pool: pool.clone(),
    });
    Some((app, pool))
}

async fn send(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = if let Some(v) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(v.to_string())
    } else {
        Body::empty()
    };
    let res = app.oneshot(builder.body(req_body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    (status, bytes.to_vec())
}

fn json_body(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(json!({}))
}

async fn mk_case(pool: &PgPool, tag: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO court_cases (case_id, docket_number, jurisdiction)
         VALUES (gen_random_uuid(), $1, 'ZZ-TEST') RETURNING case_id",
    )
    .bind(format!("meta-test-{tag}-{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn transparency_snapshot_and_inclusion_proof() {
    let Some((app, pool)) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (st, body) = send(app.clone(), "POST", "/transparency/snapshot", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let snap = json_body(&body);
    assert_eq!(snap["merkle_root"].as_str().unwrap().len(), 64);
    assert!(snap["tree_size"].as_u64().unwrap() >= 2); // chain leaves

    let (st, body) = send(app.clone(), "GET", "/transparency/snapshots/latest", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["id"], snap["id"]);

    // Chain leaf inclusion proof verifies against the snapshot root.
    let (st, body) = send(
        app.clone(),
        "GET",
        "/transparency/proof/__chain__/ledger_head",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let proof = json_body(&body);
    assert_eq!(proof["proof"]["tree_size"], snap["tree_size"]);

    let (st, body) = send(
        app.clone(),
        "POST",
        "/transparency/verify",
        Some(json!({
            "proof": proof["proof"],
            "root": snap["merkle_root"],
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["valid"], true);

    // Tampered leaf must fail; malformed root is valid:false, never an error.
    let mut forged = proof["proof"].clone();
    forged["leaf_hash"][0] = json!(0u8); // flip the first byte of the 32-byte array
    let (st, body) = send(
        app.clone(),
        "POST",
        "/transparency/verify",
        Some(json!({ "proof": forged, "root": snap["merkle_root"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json_body(&body)["valid"], false);
    let (st, body) = send(
        app.clone(),
        "POST",
        "/transparency/verify",
        Some(json!({ "proof": proof["proof"], "root": "zz" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json_body(&body)["valid"], false);

    // A row that was never snapshotted is 404, not a fabricated proof.
    let (st, _) = send(app, "GET", "/transparency/proof/findings/no-such-row", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let _ = pool; // fixture pool kept for parity with other tests
}

#[tokio::test]
async fn resonance_compute_and_case_detail() {
    let Some((app, pool)) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    // Seed: 10 cases, one with three pending high-severity flags.
    let mut ids = Vec::new();
    for i in 0..10 {
        let id = mk_case(&pool, &format!("res{i}")).await;
        for _ in 0..(if i == 0 { 3 } else { 1 }) {
            sqlx::query(
                "INSERT INTO abuse_flags (case_id, label, severity)
                 VALUES ($1, 'meta-test', 'high')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        ids.push(id);
    }

    let (st, body) = send(app.clone(), "POST", "/resonance/compute", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let report = json_body(&body);
    assert!(report["scored"].as_u64().unwrap() >= 10);

    let (st, body) = send(
        app.clone(),
        "GET",
        &format!("/resonance/case/{}", ids[0]),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let hot = json_body(&body);
    assert_eq!(hot["status"], "pending");
    assert_eq!(hot["signals"]["machine_derived"], true);

    let (st, body) = send(app.clone(), "GET", "/resonance/cases?max_q=1.0", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(json_body(&body)["cases"].as_array().unwrap().len() >= 10);

    // Unknown case → 404.
    let (st, _) = send(
        app,
        "GET",
        &format!("/resonance/case/{}", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

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

#[tokio::test]
async fn drift_and_capture_routes() {
    let Some((app, pool)) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (st, body) = send(app.clone(), "POST", "/drift/ingest", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let ingest = json_body(&body);
    assert!(ingest["rows_scanned"].as_u64().is_some());

    // Too few observations → OK with zero changepoints (gap is visible).
    let (st, body) = send(
        app.clone(),
        "POST",
        "/drift/detect/zz-meta/amend.04.search",
        Some(json!({"hazard": 50.0})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["changepoints"], json!([]));

    // Invalid hazard is a 400.
    let (st, _) = send(
        app.clone(),
        "POST",
        "/drift/detect/zz-meta/amend.04.search",
        Some(json!({"hazard": 0.0})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    let (st, body) = send(app.clone(), "GET", "/drift/changepoints?min_posterior=0.9", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(json_body(&body)["changepoints"].is_array());

    let (st, body) = send(app.clone(), "POST", "/capture/rebuild", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(json_body(&body)["opinions_scanned"].as_u64().is_some());

    // Too few permutations is a 400, not a silent degradation.
    let (st, _) = send(
        app.clone(),
        "POST",
        "/capture/compute",
        Some(json!({"permutations": 10, "seed": 1})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    let (st, body) = send(
        app.clone(),
        "POST",
        "/capture/compute",
        Some(json!({"permutations": 1000, "seed": 42})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let report = json_body(&body);
    assert_eq!(report["permutations"], 1000);
    assert_eq!(report["seed"], 42);

    let (st, body) = send(app, "GET", "/capture/outliers?max_p=0.05", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(json_body(&body)["outliers"].is_array());

    let _ = pool;
}
