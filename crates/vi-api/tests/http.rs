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
    assert_eq!(catalog["engines"].as_array().unwrap().len(), 14);

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
async fn reckoning_engine_is_gated_and_evidence_backed() {
    let Some(app) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    const DEMO_ACTOR: &str = "aaaaaaaa-1111-4111-8111-111111111111";
    const JUDGE_ACTOR: &str = "aaaaaaaa-5555-4555-8555-555555555555";

    let (st, body) = send(app.clone(), "GET", "/reckoning/wall", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let wall = json_body(&body);
    assert_eq!(wall["name"], "Public Accountability Register");
    assert_eq!(wall["charges"], false);
    assert!(
        wall["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["actor_id"] == DEMO_ACTOR
                && e["public_records"].as_array().unwrap().iter().any(|r| {
                    r["finding_type"] == "brady" && r["citation"] == "Demo v. Demo (2024)"
                })),
        "counsel-substantiated public-record findings publish without a second opt-in"
    );
    assert!(!wall["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["actor_id"] == JUDGE_ACTOR));

    let (st, body) = send(app.clone(), "GET", "/reckoning/statutes", None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(json_body(&body)["statutes"].as_array().unwrap().len() >= 5);

    let (st, body) = send(
        app.clone(),
        "POST",
        "/reckoning/resolve",
        Some(json!({
            "role": "prosecutor",
            "name": "Demo Prosecutor",
            "jurisdiction": "CA",
            "bar_number": "CA-100001"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let hit = json_body(&body);
    assert_eq!(hit["actor"]["actor_id"], DEMO_ACTOR);
    assert_eq!(hit["method"], "bar_number");

    let (st, body) = send(
        app.clone(),
        "GET",
        &format!("/reckoning/actors/{DEMO_ACTOR}/score"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let score = json_body(&body);
    assert!(score["score"].as_f64().unwrap() >= 28.0);
    assert!(score["substantiated_findings"].as_i64().unwrap() >= 1);

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/actors/{DEMO_ACTOR}/package"),
        Some(json!({"kind": "criminal_referral"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let pkg = json_body(&body);
    let md = pkg["markdown"].as_str().unwrap();
    assert!(md.contains("Attorney Work Product"));
    assert!(md.contains("not a charging document"));
    assert!(md.contains("18 U.S.C."));
    assert!(md.contains("statutory maximum"));
    assert!(!md.to_lowercase().contains("no mercy"));

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/actors/{JUDGE_ACTOR}/publish"),
        Some(json!({"approved": true})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "{}",
        String::from_utf8_lossy(&body)
    );

    let (st, body) = send(
        app.clone(),
        "GET",
        &format!("/reckoning/wall/{DEMO_ACTOR}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["entry"]["display_name"], "Demo Prosecutor");

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/actors/{DEMO_ACTOR}/publish"),
        Some(json!({"approved": false, "notes": "hold for victim-privacy review"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["effect"], "hold");

    let (st, body) = send(app.clone(), "GET", "/reckoning/wall", None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(!json_body(&body)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["actor_id"] == DEMO_ACTOR));

    let (st, body) = send(
        app,
        "POST",
        &format!("/reckoning/actors/{DEMO_ACTOR}/publish"),
        Some(json!({"approved": true, "notes": "hold lifted"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
}

#[tokio::test]
async fn constitution_engine_is_national() {
    let Some(app) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (st, body) = send(app.clone(), "GET", "/constitution", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let cat = json_body(&body);
    assert_eq!(cat["states"], 50);
    assert_eq!(cat["amendments"], 27);
    assert!(cat["provision_rows"].as_i64().unwrap() >= 40);

    let (st, body) = send(
        app.clone(),
        "GET",
        "/constitution/jurisdictions?kind=state",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let jurs = json_body(&body);
    assert_eq!(jurs["jurisdictions"].as_array().unwrap().len(), 50);

    let (st, body) = send(
        app.clone(),
        "GET",
        "/constitution/provisions/amend.04",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(json_body(&body)["body"]
        .as_str()
        .unwrap()
        .contains("unreasonable searches"));

    let (st, body) = send(
        app.clone(),
        "POST",
        "/constitution/resolve",
        Some(json!({
            "clause_id": "amend.04.search_seizure",
            "jurisdiction": "CA",
            "court_level": "superior"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let resolved = json_body(&body);
    assert_eq!(resolved["circuit"], "CA9");
    assert_eq!(resolved["state_analog"]["state_above_federal"], true);

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/constitution/screen/{DEMO_CASE}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let screen = json_body(&body);
    assert!(screen["hit_count"].as_u64().unwrap() >= 2);

    let (st, body) = send(
        app,
        "GET",
        &format!("/constitution/screen/{DEMO_CASE}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let md = String::from_utf8(body).unwrap();
    assert!(md.contains("Attorney Work Product"));
    assert!(md.contains("not legal advice"));
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
