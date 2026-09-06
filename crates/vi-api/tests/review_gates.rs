//! Integration tests for the E2E wiring gates: flag review, package
//! transitions, screen review, unresolved close-out, the full-engine pipeline
//! report, and public case redaction. Skipped unless DATABASE_URL is set.
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

const DEMO_CASE: &str = "22222222-2222-2222-2222-222222222222";
const DEMO_ACTOR: &str = "aaaaaaaa-1111-4111-8111-111111111111";

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

async fn app_and_pool() -> Option<(axum::Router, PgPool)> {
    let pool = pool().await?;
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

/// A fully-specified test case row (plus prosecutor/office), isolated from the
/// seed set and from other tests by random ids.
async fn fresh_case(pool: &PgPool, with_disposition: bool) -> (Uuid, Uuid, String) {
    let case_id = Uuid::new_v4();
    let prosecutor_id = Uuid::new_v4();
    let office = format!("Wiring Test Office {case_id}");
    sqlx::query(
        "INSERT INTO prosecutors (prosecutor_id, name, office, jurisdiction)
         VALUES ($1, 'Wiring Test Prosecutor', $2, 'CA')",
    )
    .bind(prosecutor_id)
    .bind(&office)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO court_cases (
            case_id, docket_number, jurisdiction, court_level, charge_category,
            prosecutor_id, judge, evidence_strength, outcome,
            plea_offered, plea_accepted, plea_offer_months, sentence_months, charges
         ) VALUES ($1, $2, 'CA', 'superior', 'drug', $3, 'Wiring Test J.', 'mixed',
                   $4, $5, $6, $7, $8, $9)",
    )
    .bind(case_id)
    .bind(format!("TEST-{case_id}"))
    .bind(prosecutor_id)
    .bind(with_disposition.then_some("conviction"))
    .bind(with_disposition.then_some(true))
    .bind(with_disposition.then_some(false))
    .bind(with_disposition.then_some(12))
    .bind(with_disposition.then_some(12))
    .bind(if with_disposition {
        Some(vec!["possession".to_string(), "distribution".to_string()])
    } else {
        None
    })
    .execute(pool)
    .await
    .unwrap();
    (case_id, prosecutor_id, office)
}

#[tokio::test]
async fn flag_review_gate_flips_score_inputs() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());
    let (case_id, prosecutor_id, _office) = fresh_case(&pool, true).await;

    // An actor resolved to the test prosecutor, so substantiation has someone
    // to rescore.
    let actor_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accountability_actors
           (actor_id, role, display_name, normalized_name, jurisdiction, prosecutor_id, fingerprint)
         VALUES ($1, 'prosecutor', 'Wiring Test Prosecutor', 'wiring test prosecutor', 'CA', $2, $3)",
    )
    .bind(actor_id)
    .bind(prosecutor_id)
    .bind(format!("prosecutor|wiring test prosecutor|ca|{actor_id}|"))
    .execute(&pool)
    .await
    .unwrap();

    // A rule that fires on this case (evidence_strength == 'mixed').
    let rule_name = format!("review-gate-{case_id}");
    let (st, body) = send(
        app.clone(),
        "POST",
        "/rules",
        Some(json!({
            "name": rule_name,
            "source": "when case.evidence_strength == \"mixed\" then flag \"Review gate test\" severity low",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));

    let (st, body) = send(
        app.clone(),
        "POST",
        "/rules/run",
        Some(json!({ "case_id": case_id })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let flags = json_body(&body);
    let flag_id = flags["flags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"] == "Review gate test")
        .and_then(|f| f["flag_id"].as_str())
        .expect("rule should have fired a pending flag")
        .to_string();

    // Pending flags contribute nothing to the score.
    let before = vi_reckoning::score_actor(&pool, None, actor_id).await.unwrap();
    assert_eq!(before.score, 0.0, "pending flag must not score");
    assert_eq!(before.substantiated_flags, 0);

    // Substantiate: the gate flips, the linked actor is rescored, and the
    // transition hits the ledger.
    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/flags/{flag_id}/review"),
        Some(json!({ "action": "substantiate", "notes": "verified against the record" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let reviewed = json_body(&body);
    assert_eq!(reviewed["review_status"], "substantiated");
    let rescored = reviewed["rescored_actors"].as_array().unwrap();
    assert!(
        rescored.iter().any(|r| r["actor_id"] == json!(actor_id)),
        "linked actor should be rescored on substantiation: {rescored:?}"
    );

    let after = vi_reckoning::score_actor(&pool, None, actor_id).await.unwrap();
    assert!(
        after.score > before.score,
        "substantiated flag must raise the score ({} -> {})",
        before.score,
        after.score
    );
    assert_eq!(after.substantiated_flags, 1);

    let (status, reviewed_at, notes): (String, Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
        sqlx::query_as("SELECT review_status, reviewed_at, review_notes FROM abuse_flags WHERE flag_id = $1")
            .bind(Uuid::parse_str(&flag_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "substantiated");
    assert!(reviewed_at.is_some());
    assert_eq!(notes.as_deref(), Some("verified against the record"));

    // Reject path on a second flag from a second rule.
    let rule_name = format!("review-gate-reject-{case_id}");
    send(
        app.clone(),
        "POST",
        "/rules",
        Some(json!({
            "name": rule_name,
            "source": "when case.outcome == \"conviction\" then flag \"Reject path test\" severity low",
        })),
    )
    .await;
    let (_, body) = send(
        app.clone(),
        "POST",
        "/rules/run",
        Some(json!({ "case_id": case_id })),
    )
    .await;
    let flags = json_body(&body);
    let reject_flag = flags["flags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"] == "Reject path test")
        .and_then(|f| f["flag_id"].as_str())
        .unwrap()
        .to_string();
    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/flags/{reject_flag}/review"),
        Some(json!({ "action": "reject", "notes": "not supported by the record" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["review_status"], "rejected");
    let final_score = vi_reckoning::score_actor(&pool, None, actor_id).await.unwrap();
    assert_eq!(
        final_score.substantiated_flags, 1,
        "a rejected flag must not count"
    );

    // Bad action and unknown flag.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/flags/{flag_id}/review"),
        Some(json!({ "action": "publish" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/flags/{}/review", Uuid::new_v4()),
        Some(json!({ "action": "substantiate" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // The review transitions are on the hash chain, and the chain verifies.
    let events = ledger.entries_for_case(case_id).await.unwrap();
    assert!(events
        .iter()
        .any(|e| e.event_type == vi_ledger::events::FLAG_REVIEWED));
    let report = ledger.verify().await.unwrap();
    assert!(report.ok, "ledger chain must verify: {report:?}");
}

#[tokio::test]
async fn package_state_machine_rejects_illegal_transitions() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/actors/{DEMO_ACTOR}/package"),
        Some(json!({ "kind": "bar_complaint" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let pkg = json_body(&body);
    assert_eq!(pkg["status"], "draft");
    let package_id = pkg["package_id"].as_str().unwrap();

    // No skipping: draft -> referred is illegal.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({ "to": "referred" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Unknown target.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({ "to": "published" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Legal step forward.
    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({ "to": "attorney_reviewed", "notes": "read and approved" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["status"], "attorney_reviewed");

    // No regression and no re-applying the same step.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({ "to": "attorney_reviewed" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({ "to": "referred", "notes": "sent to the bar" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["status"], "referred");

    // Terminal for this machine: referred -> anything is illegal.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{package_id}/transition"),
        Some(json!({ "to": "attorney_reviewed" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Unknown package.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/packages/{}/transition", Uuid::new_v4()),
        Some(json!({ "to": "attorney_reviewed" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // The tracker surfaces attorney-reviewed and referred packages.
    let (st, body) = send(app.clone(), "GET", "/reckoning/tracker", None).await;
    assert_eq!(st, StatusCode::OK);
    let tracker = json_body(&body);
    assert!(
        tracker["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["package_id"] == json!(package_id)),
        "referred package should appear on the tracker"
    );

    let actor = Uuid::parse_str(DEMO_ACTOR).unwrap();
    let events = ledger.entries_for_actor(actor).await.unwrap();
    assert!(events
        .iter()
        .filter(|e| e.event_type == vi_ledger::events::PACKAGE_TRANSITION)
        .count()
        >= 2);
    let report = ledger.verify().await.unwrap();
    assert!(report.ok, "ledger chain must verify: {report:?}");
}

#[tokio::test]
async fn constitution_screen_review_gate() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/constitution/screen/{DEMO_CASE}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let screen_id = json_body(&body)["screen_id"].as_str().unwrap().to_string();

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/constitution/screens/{screen_id}/review"),
        Some(json!({ "action": "substantiate", "notes": "hits verified" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["review_status"], "substantiated");

    let (status, reviewed_at): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT review_status, reviewed_at FROM constitution_screens WHERE screen_id = $1",
    )
    .bind(Uuid::parse_str(&screen_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "substantiated");
    assert!(reviewed_at.is_some());

    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/constitution/screens/{screen_id}/review"),
        Some(json!({ "action": "publish" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/constitution/screens/{}/review", Uuid::new_v4()),
        Some(json!({ "action": "reject" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let case = Uuid::parse_str(DEMO_CASE).unwrap();
    let events = ledger.entries_for_case(case).await.unwrap();
    assert!(events
        .iter()
        .any(|e| e.event_type == vi_ledger::events::SCREEN_REVIEWED));
    let report = ledger.verify().await.unwrap();
    assert!(report.ok);
}

#[tokio::test]
async fn unresolved_official_close_out() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    let unresolved_id = Uuid::new_v4();
    let case = Uuid::parse_str(DEMO_CASE).unwrap();
    sqlx::query(
        "INSERT INTO unresolved_officials
           (unresolved_id, case_id, role_in_case, raw_value, reason_kind, reason)
         VALUES ($1, $2, 'judge', $3, 'ambiguous', 'test: unreadable name boundary')",
    )
    .bind(unresolved_id)
    .bind(case)
    .bind(format!("Smith, John {unresolved_id}"))
    .execute(&pool)
    .await
    .unwrap();

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/unresolved/{unresolved_id}/resolve"),
        Some(json!({ "resolution": "not_identifiable", "notes": "record does not say" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["resolution"], "not_identifiable");

    // Already closed: 409, not a silent overwrite.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/unresolved/{unresolved_id}/resolve"),
        Some(json!({ "resolution": "identified" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/reckoning/unresolved/{}/resolve", Uuid::new_v4()),
        Some(json!({ "resolution": "identified" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let (resolved, notes): (Option<chrono::DateTime<chrono::Utc>>, Option<String>) = sqlx::query_as(
        "SELECT resolved_at, resolution_notes FROM unresolved_officials WHERE unresolved_id = $1",
    )
    .bind(unresolved_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(resolved.is_some());
    assert_eq!(notes.as_deref(), Some("record does not say"));

    let events = ledger.entries_for_case(case).await.unwrap();
    assert!(events
        .iter()
        .any(|e| e.event_type == vi_ledger::events::OFFICIAL_RESOLVED));
    let report = ledger.verify().await.unwrap();
    assert!(report.ok);
}

#[tokio::test]
async fn pipeline_report_counts_every_stage() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    // A case with everything: text mentioning a doctrine, two charges, and
    // full disposition fields.
    let (full_case, _pid, office) = fresh_case(&pool, true).await;
    sqlx::query(
        "INSERT INTO court_opinions
           (opinion_id, case_id, court_level, judge, citation, date_issued, full_text)
         VALUES ($1, $2, 'superior', 'Wiring Test J.', 'Wiring Test (2026)', '2026-01-01',
                 'The Brady material was withheld. Discovery violation alleged at trial. \
                  The suppression motion is denied and the conviction is affirmed.')",
    )
    .bind(Uuid::new_v4())
    .bind(full_case)
    .execute(&pool)
    .await
    .unwrap();

    // A case with nothing: no opinion, no charges, no disposition, no office.
    let bare_case = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO court_cases (case_id, docket_number, jurisdiction)
         VALUES ($1, $2, 'unknown')",
    )
    .bind(bare_case)
    .bind(format!("TEST-BARE-{bare_case}"))
    .execute(&pool)
    .await
    .unwrap();

    let (st, body) = send(
        app.clone(),
        "POST",
        "/pipeline/run",
        Some(json!({ "case_id": full_case })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let summary = json_body(&body);
    assert!(summary["report_id"].is_string(), "report row id returned");
    let counts = &summary["stage_counts"];
    for stage in [
        "forum",
        "constitution_screen",
        "evidence_leads",
        "abuse_rules",
        "actor_links",
        "score",
        "tactics_occurrences",
        "trial_penalty",
        "monell_refresh",
        "sim_calibration",
        "correlation_refresh",
        "drift_ingest_signals",
        "drift_detect",
        "capture_rebuild",
        "capture_metrics",
        "resonance_compute",
    ] {
        assert!(
            counts.get(stage).is_some(),
            "stage '{stage}' must be counted even when it ran zero times"
        );
    }
    assert_eq!(counts["tactics_occurrences"]["processed"], 1);
    assert_eq!(counts["trial_penalty"]["processed"], 1);
    assert_eq!(counts["monell_refresh"]["processed"], 1);
    assert_eq!(counts["sim_calibration"]["processed"], 1);
    // Meta-engine stages: the opinion carries a lexicon outcome signal and an
    // author, so drift ingestion, capture rebuild/metrics, and resonance all
    // processed this case.
    assert_eq!(counts["drift_ingest_signals"]["processed"], 1);
    assert_eq!(counts["capture_rebuild"]["processed"], 1);
    assert_eq!(counts["capture_metrics"]["processed"], 1);
    assert_eq!(counts["resonance_compute"]["processed"], 1);
    // One paired observation is not a correlation: reported, not fabricated.
    let corr_stage = summary["runs"][0]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["stage"] == "correlation_refresh")
        .unwrap();
    assert_eq!(corr_stage["detail"]["plea_sentence_n"], 1);
    assert!(corr_stage["detail"]["plea_sentence_r"].is_null());

    // Occurrences are pending leads; the doctrine mentions were recorded.
    let occ: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tactic_occurrences WHERE case_id = $1 AND review_status = 'pending'",
    )
    .bind(full_case)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(occ >= 3, "brady + discovery + charge_stack occurrences, got {occ}");

    // The stored sim stats the /simulate/from-case route now prefers.
    let stored: Option<(Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT conviction_rate, plea_sentence_r FROM office_sim_stats WHERE office = $1",
    )
    .bind(&office)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (conviction_rate, plea_r) = stored.expect("office stats row");
    assert_eq!(conviction_rate, Some(1.0));
    assert_eq!(plea_r, None);

    // The bare case: skips are counted, not dropped.
    let (st, body) = send(
        app.clone(),
        "POST",
        "/pipeline/run",
        Some(json!({ "case_id": bare_case })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let summary = json_body(&body);
    let counts = &summary["stage_counts"];
    assert_eq!(counts["tactics_occurrences"]["skipped_no_data"], 1);
    assert_eq!(counts["trial_penalty"]["skipped_no_data"], 1);
    assert_eq!(counts["monell_refresh"]["skipped_no_data"], 1);
    assert_eq!(counts["sim_calibration"]["skipped_no_data"], 1);
    assert_eq!(counts["correlation_refresh"]["skipped_no_data"], 1);
    assert_eq!(counts["drift_ingest_signals"]["skipped_no_data"], 1);
    assert_eq!(counts["drift_detect"]["skipped_no_data"], 1);
    assert_eq!(counts["capture_rebuild"]["skipped_no_data"], 1);
    // Resonance always computes: an empty corpus is an honest empty report.
    assert_eq!(counts["resonance_compute"]["processed"], 1);

    // The report row is durable and shows up in pipeline status.
    let report_id = summary["report_id"].as_str().unwrap();
    let persisted: Option<String> = sqlx::query_scalar(
        "SELECT stage_counts->'trial_penalty'->>'skipped_no_data' FROM pipeline_reports WHERE report_id = $1",
    )
    .bind(Uuid::parse_str(report_id).unwrap())
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(persisted.as_deref(), Some("1"));

    let (st, body) = send(app.clone(), "GET", "/pipeline/status", None).await;
    assert_eq!(st, StatusCode::OK);
    let status = json_body(&body);
    assert_eq!(status["stages"].as_array().unwrap().len(), 16);
    assert!(status["latest_report"]["stage_counts"].is_object());

    let report = ledger.verify().await.unwrap();
    assert!(report.ok, "ledger chain must verify: {report:?}");
}

#[tokio::test]
async fn case_detail_redacts_defendant_race() {
    let Some((app, _pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (st, body) = send(app, "GET", &format!("/cases/{DEMO_CASE}"), None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let raw = String::from_utf8_lossy(&body);
    let case = &json_body(&body)["case"];
    assert!(
        case.get("defendant_race").is_none(),
        "raw defendant_race must not appear in the public serialization"
    );
    let pseudonym = case["defendant_race_pseudonym"]
        .as_str()
        .expect("pseudonym replaces the raw value");
    assert_eq!(pseudonym.len(), 16);
    // The seed value for this case is "Black" (0004_seed.sql); it must not
    // appear anywhere in the response.
    assert!(!raw.contains("\"Black\""), "raw race value leaked: {raw}");
}

#[tokio::test]
async fn sim_from_case_uses_stored_office_stats() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (case_id, _pid, office) = fresh_case(&pool, true).await;

    // Before the pipeline refreshes the office: no stored stats, and the sim
    // still computes honestly from live data (one conviction → calibrated).
    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/simulate/from-case/{case_id}"),
        Some(json!({ "trials": 1000, "seed": 7 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["priors_source"], "calibrated");

    // After the pipeline's sim_calibration stage stores the aggregates, the
    // sim reads the stored row — same priors, now from the refreshed stats.
    let (st, body) = send(
        app.clone(),
        "POST",
        "/pipeline/run",
        Some(json!({ "case_id": case_id })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let stored: Option<i64> =
        sqlx::query_scalar("SELECT cases_with_outcome::bigint FROM office_sim_stats WHERE office = $1")
            .bind(&office)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(stored, Some(1));

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/simulate/from-case/{case_id}"),
        Some(json!({ "trials": 1000, "seed": 7 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let sim = json_body(&body);
    assert_eq!(sim["priors_source"], "calibrated");
    assert_eq!(sim["priors"]["prosecutor_aggressiveness"], 1.0);
}