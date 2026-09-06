//! HTTP integration tests for the counsel review gates and extended pipeline
//! stages. Skipped unless DATABASE_URL is set.
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

/// A case with an opinion that triggers the tactic catalog's Brady entry.
async fn mk_gate_case(pool: &PgPool) -> Uuid {
    let case_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO court_cases (case_id, docket_number, jurisdiction, judge,
                                  plea_offered, plea_accepted, outcome,
                                  plea_offer_months, sentence_months,
                                  charge_category)
         VALUES ($1, $2, 'CA', 'Gate, Q.', true, false, 'conviction', 12, 36, 'drug')",
    )
    .bind(case_id)
    .bind(format!("gate-test-{}", &case_id.to_string()[..8]))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO court_opinions
            (opinion_id, case_id, judge, date_issued, full_text, source_ref)
         VALUES (gen_random_uuid(), $1, 'Gate, Q.', DATE '2024-04-01',
                 'The court noted the Brady disclosure issue and the suppression hearing.',
                 $2)",
    )
    .bind(case_id)
    .bind(format!("gate-test:{case_id}"))
    .execute(pool)
    .await
    .unwrap();
    case_id
}

async fn cleanup(pool: &PgPool, case_id: Uuid) {
    sqlx::query("DELETE FROM tactic_occurrences WHERE case_id = $1")
        .bind(case_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM trial_penalty_observations WHERE case_id = $1")
        .bind(case_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM constitution_screens WHERE case_id = $1")
        .bind(case_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM abuse_flags WHERE case_id = $1")
        .bind(case_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM court_cases WHERE case_id = $1")
        .bind(case_id)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn flag_review_gate_substantiate_and_reject() {
    let Some((app, pool)) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let case_id = mk_gate_case(&pool).await;
    let flag_id: Uuid = sqlx::query_scalar(
        "INSERT INTO abuse_flags (case_id, label, severity)
         VALUES ($1, 'gate-test', 'high') RETURNING flag_id",
    )
    .bind(case_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Unknown action is a 400.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/flags/{flag_id}/review"),
        Some(json!({"action": "shrug"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Substantiate: reviewed_at is set and a ledger event lands.
    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/flags/{flag_id}/review"),
        Some(json!({"action": "substantiate", "notes": "confirmed on the record"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["review_status"], "substantiated");
    let reviewed: (Option<chrono::DateTime<chrono::Utc>>, Option<String>, String) =
        sqlx::query_as(
            "SELECT reviewed_at, review_notes, review_status FROM abuse_flags WHERE flag_id = $1",
        )
        .bind(flag_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(reviewed.0.is_some());
    assert_eq!(reviewed.1.as_deref(), Some("confirmed on the record"));
    assert_eq!(reviewed.2, "substantiated");

    // Reject path on a second flag.
    let flag2: Uuid = sqlx::query_scalar(
        "INSERT INTO abuse_flags (case_id, label, severity)
         VALUES ($1, 'gate-test-2', 'low') RETURNING flag_id",
    )
    .bind(case_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/flags/{flag2}/review"),
        Some(json!({"action": "reject"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["review_status"], "rejected");

    // Unknown flag id is a 404, not a silent no-op.
    let (st, _) = send(
        app,
        "POST",
        &format!("/flags/{}/review", Uuid::new_v4()),
        Some(json!({"action": "reject"})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    cleanup(&pool, case_id).await;
}

#[tokio::test]
async fn constitution_screen_review_gate() {
    let Some((app, pool)) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let case_id = mk_gate_case(&pool).await;

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/constitution/screen/{case_id}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let screen_id: Uuid =
        serde_json::from_value(json_body(&body)["screen_id"].clone()).unwrap();

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/constitution/screens/{screen_id}/review"),
        Some(json!({"action": "substantiate", "notes": "checked against filings"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["review_status"], "substantiated");
    let status: (String,) = sqlx::query_as(
        "SELECT review_status FROM constitution_screens WHERE screen_id = $1",
    )
    .bind(screen_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.0, "substantiated");

    let (st, _) = send(
        app,
        "POST",
        &format!("/constitution/screens/{screen_id}/review"),
        Some(json!({"action": "shrug"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    cleanup(&pool, case_id).await;
}

#[tokio::test]
async fn package_transition_state_machine() {
    let Some((app, pool)) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    const DEMO_ACTOR: &str = "aaaaaaaa-1111-4111-8111-111111111111";

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/actors/{DEMO_ACTOR}/package"),
        Some(json!({"kind": "bar_complaint"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let package_id: Uuid =
        serde_json::from_value(json_body(&body)["package_id"].clone()).unwrap();

    // Skipping attorney review is a 409 conflict, not a shortcut.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({"to": "referred"})),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({"to": "attorney_reviewed", "notes": "read"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["status"], "attorney_reviewed");

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({"to": "referred", "notes": "sent to the bar"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["status"], "referred");

    // Referred is terminal; further transitions conflict.
    let (st, _) = send(
        app,
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({"to": "attorney_reviewed"})),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    let _ = pool;
}

#[tokio::test]
async fn pipeline_extended_stages_produce_pending_artifacts() {
    let Some((app, pool)) = router().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let case_id = mk_gate_case(&pool).await;

    let (st, body) = send(
        app.clone(),
        "POST",
        "/pipeline/run",
        Some(json!({"case_id": case_id})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let summary = json_body(&body);
    assert!(summary["cases_processed"].as_u64().unwrap() >= 1);

    // Tactic occurrences: the Brady signal in the opinion text was matched,
    // pending review — an observation, not an accusation.
    let occ: Vec<(String,)> = sqlx::query_as(
        "SELECT review_status FROM tactic_occurrences WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!occ.is_empty(), "tactic_occurrences stage ran");
    assert!(occ.iter().all(|(s,)| s == "pending"));

    // Trial-penalty accumulation: the case carried disposition fields, so it
    // landed in the observations table.
    let obs: Option<(bool,)> = sqlx::query_as(
        "SELECT plea_offered FROM trial_penalty_observations WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(obs, Some((true,)));

    // The pipeline report row exists with per-stage counts.
    let reports: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pipeline_reports WHERE trigger = 'operator'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(reports >= 1);

    cleanup(&pool, case_id).await;
}
