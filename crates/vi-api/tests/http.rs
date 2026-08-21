//! HTTP integration tests. Skipped unless DATABASE_URL is set (CI sets it).
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use vi_api::handlers::AppState;
use vi_ledger::Ledger;

const DEMO_CASE: &str = "22222222-2222-2222-2222-222222222222";
const BRADY_TACTIC: &str = "aaaaaaaa-0000-4000-8000-000000000001";

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

async fn router() -> Option<axum::Router> {
    let pool = pool().await?;
    vi_db::migrate(&pool).await.ok()?;
    let _ = vi_api::handlers::backfill_h3(&pool).await;
    Some(vi_api::router(AppState {
        ledger: Ledger::new(pool.clone()),
        pool,
    }))
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

#[tokio::test]
async fn all_engines_wired_over_http() {
    let Some(app) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (st, body) = send(app.clone(), "GET", "/engines", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let catalog = json_body(&body);
    assert_eq!(catalog["backend"], "vi-api");
    assert_eq!(catalog["engines"].as_array().unwrap().len(), 12);

    let (st, body) = send(app.clone(), "GET", "/tactics", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(json_body(&body)["tactics"].as_array().unwrap().len() >= 7);

    let (st, body) = send(
        app.clone(),
        "GET",
        &format!("/tactics/{BRADY_TACTIC}/stats"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let stats = json_body(&body);
    assert!(stats["data_points"].as_i64().unwrap() >= 1);
    assert!(stats["hits"].as_i64().unwrap() >= 1);

    let (st, body) = send(
        app.clone(),
        "POST",
        "/ingest/run",
        Some(json!({"source": "fixture"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let ingest = json_body(&body);
    assert_eq!(ingest["source"], "fixture");
    assert!(ingest["cases_persisted"].as_u64().unwrap() >= 1);
    assert!(ingest["opinions_persisted"].as_u64().unwrap() >= 1);

    let (st, body) = send(app.clone(), "GET", "/ingest/status", None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(json_body(&body)["cursors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["source"] == "fixture"));

    let cell = vi_geo::cell_for(37.7749, -122.4194, 8).unwrap();
    let (st, body) = send(app.clone(), "GET", &format!("/geo/cells/{cell}"), None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let geo = json_body(&body);
    assert!(geo["boundary"].as_array().unwrap().len() >= 6);

    let (st, body) = send(app.clone(), "GET", &format!("/geo/kring/{cell}?k=1"), None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let kring = json_body(&body);
    assert!(!kring["cells"].as_array().unwrap().is_empty());
    assert!(kring["cells"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["origin"] == true && c["total"].as_i64().unwrap() >= 1));

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/simulate/from-case/{DEMO_CASE}"),
        Some(json!({"trials": 1000, "seed": 7})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let sim = json_body(&body);
    assert_eq!(sim["priors_source"], "calibrated");
    assert!(sim["distribution"]["trials"].as_u64().unwrap() >= 1000);

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/brady/reconcile/{DEMO_CASE}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));

    let (st, body) = send(
        app.clone(),
        "GET",
        &format!("/lasm/package/{DEMO_CASE}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let md = String::from_utf8(body).unwrap();
    assert!(md.contains("Attorney Work Product"));
    assert!(md.contains("research leads"));
    assert!(md.contains("Trial-Penalty"));
}

#[tokio::test]
async fn unknown_ingest_source_is_bad_request() {
    let Some(app) = router().await else {
        return;
    };
    let (st, body) = send(app, "POST", "/ingest/run", Some(json!({"source": "pacer"}))).await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "{}",
        String::from_utf8_lossy(&body)
    );
}
