//! Integration tests for the transparency / resonance / drift / capture
//! routes and the pipeline stages that feed them. Skipped unless DATABASE_URL
//! is set.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use chrono::NaiveDate;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
use vi_api::handlers::AppState;
use vi_ledger::Ledger;

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
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, bytes.to_vec())
}

fn json_body(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(json!({}))
}

#[tokio::test]
async fn transparency_snapshot_proof_and_verify() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    // Trigger a snapshot; it must anchor in the ledger.
    let (st, body) = send(app.clone(), "POST", "/transparency/snapshot", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let snap = json_body(&body);
    let snap_id = snap["id"].as_str().unwrap();
    let root = snap["merkle_root"].as_str().unwrap().to_string();
    assert_eq!(root.len(), 64, "hex-encoded 32-byte root");
    assert!(snap["tree_size"].as_i64().unwrap() > 0);
    assert!(snap["table_counts"]["ledger_events"].as_i64().unwrap() >= 1);
    assert!(snap["ledger_seq"].as_i64().unwrap() >= 1);

    // A second snapshot chains to the first.
    let (st, body) = send(app.clone(), "POST", "/transparency/snapshot", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let snap2 = json_body(&body);
    assert_eq!(snap2["prev_root"].as_str().unwrap(), root);

    let (st, body) = send(app.clone(), "GET", "/transparency/snapshots", None).await;
    assert_eq!(st, StatusCode::OK);
    let list = json_body(&body);
    assert!(list["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"] == snap_id));

    let (st, body) = send(app.clone(), "GET", "/transparency/snapshots/latest", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json_body(&body)["id"], snap2["id"]);

    // Inclusion proof for a ledger event (row_id is the ledger seq).
    let (st, body) = send(
        app.clone(),
        "GET",
        "/transparency/proof/ledger_events/1",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let proof_resp = json_body(&body);
    let proof = &proof_resp["proof"];
    assert_eq!(proof["table"], "ledger_events");
    assert_eq!(proof["row_id"], "1");

    // Verify: the proof checks out against the root of the snapshot it was
    // drawn from; a wrong root fails closed.
    let snap_row: (Vec<u8>,) =
        sqlx::query_as("SELECT merkle_root FROM transparency_snapshots WHERE id = $1")
            .bind(Uuid::parse_str(proof["snapshot_id"].as_str().unwrap()).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    let proof_root: String = snap_row.0.iter().map(|b| format!("{b:02x}")).collect();
    let (st, body) = send(
        app.clone(),
        "POST",
        "/transparency/verify",
        Some(json!({ "proof": proof, "root": proof_root })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json_body(&body)["valid"], true);

    let (st, body) = send(
        app.clone(),
        "POST",
        "/transparency/verify",
        Some(json!({ "proof": proof, "root": "00".repeat(32) })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json_body(&body)["valid"], false);

    // Malformed root is a clean `false`, not a 500.
    let (st, body) = send(
        app.clone(),
        "POST",
        "/transparency/verify",
        Some(json!({ "proof": proof, "root": "not-hex" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json_body(&body)["valid"], false);

    // A row that was never snapshotted is a 404, not a fabricated proof.
    let (st, _) = send(
        app.clone(),
        "GET",
        "/transparency/proof/ledger_events/999999999",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let report = ledger.verify().await.unwrap();
    assert!(report.ok, "ledger chain must verify: {report:?}");
}

#[tokio::test]
async fn resonance_compute_and_reads() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    let (st, body) = send(app.clone(), "POST", "/resonance/compute", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let report = json_body(&body);
    assert!(report["scored"].as_u64().is_some());
    assert!(report["surfaced"].as_u64().is_some());
    assert_eq!(report["status"], "pending");

    let (st, body) = send(app.clone(), "GET", "/resonance/cases", None).await;
    assert_eq!(st, StatusCode::OK);
    let list = json_body(&body);
    assert_eq!(list["max_q"], 0.25);
    let cases = list["cases"].as_array().unwrap();
    for c in cases {
        assert!(c["q_value"].as_f64().unwrap() <= 0.25);
        assert_eq!(c["status"], "pending");
        assert_eq!(c["signals"]["machine_derived"], true);
    }

    // A case with no resonance row is a 404.
    let (st, _) = send(
        app.clone(),
        "GET",
        &format!("/resonance/case/{}", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // max_q filter is honored.
    let (st, body) = send(app.clone(), "GET", "/resonance/cases?max_q=1.0", None).await;
    assert_eq!(st, StatusCode::OK);
    let all = json_body(&body)["cases"].as_array().unwrap().len();
    assert!(all >= cases.len());

    // One ledger event per compute run.
    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ledger_entries WHERE event_type = 'ResonanceComputed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(events >= 1);
    let report = ledger.verify().await.unwrap();
    assert!(report.ok);
}

#[tokio::test]
async fn drift_ingest_detect_and_list() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    let (st, body) = send(app.clone(), "POST", "/drift/ingest", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let report = json_body(&body);
    assert!(report["rows_scanned"].as_u64().is_some());
    assert!(report["no_lexicon_hit"].as_u64().is_some());

    // A synthetic series with an obvious mid-series shift: six denials, then
    // six grants, on an isolated (court, clause) pair.
    let tag = &Uuid::new_v4().to_string()[..8];
    let court = format!("testcourt-{tag}");
    let clause = "amend.04.search_seizure";
    for i in 0..12 {
        let (signal, date) = if i < 6 {
            (0.0, NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + chrono::Days::new(i))
        } else {
            (1.0, NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + chrono::Days::new(i))
        };
        sqlx::query(
            "INSERT INTO drift_observations (court_id, clause_id, observed_at, signal, source_ref)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(&court)
        .bind(clause)
        .bind(date)
        .bind(signal)
        .bind(format!("test:{tag}:{i}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Hazard validation is a 400.
    let (st, _) = send(
        app.clone(),
        "POST",
        &format!("/drift/detect/{court}/{clause}"),
        Some(json!({ "hazard": 1.0 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/drift/detect/{court}/{clause}"),
        Some(json!({ "hazard": 50.0 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let out = json_body(&body);
    assert_eq!(out["status"], "pending");
    let cps = out["changepoints"].as_array().unwrap();
    assert!(!cps.is_empty(), "obvious shift must be detected: {out}");

    // Listed at a permissive threshold, all pending.
    let (st, body) = send(
        app.clone(),
        "GET",
        "/drift/changepoints?min_posterior=0.0",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let listed = json_body(&body)["changepoints"].as_array().unwrap().clone();
    assert!(listed
        .iter()
        .any(|c| c["court_id"] == json!(court) && c["status"] == "pending"));

    // A run with too few observations records zero changepoints, visibly.
    let (st, body) = send(
        app.clone(),
        "POST",
        &format!("/drift/detect/{court}-empty/{clause}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_body(&body)["changepoints"].as_array().unwrap().len(), 0);
    let run: Option<(bool,)> = sqlx::query_as(
        "SELECT (run_length->>'insufficient_data')::boolean FROM drift_runs
         WHERE court_id = $1 ORDER BY computed_at DESC LIMIT 1",
    )
    .bind(format!("{court}-empty"))
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(run, Some((true,)));

    let report = ledger.verify().await.unwrap();
    assert!(report.ok);
}

/// One test for both surfaces: `capture_rebuild` deletes the edge table
/// wholesale, so the route-level capture flow and the pipeline's capture
/// stages must not run concurrently.
#[tokio::test]
async fn capture_routes_and_pipeline_meta_stages() {
    let Some((app, pool)) = app_and_pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let ledger = Ledger::new(pool.clone());

    let (st, body) = send(app.clone(), "POST", "/capture/rebuild", None).await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let report = json_body(&body);
    assert!(report["opinions_scanned"].as_u64().is_some());
    assert!(report["edges"].as_u64().is_some());

    // A synthetic, strongly concentrated entity: one judge, six appearances,
    // every outcome against the movant. Isolated by a unique name.
    let tag = &Uuid::new_v4().to_string()[..8];
    let judge = format!("Capture Test Judge {tag}");
    for i in 0..6 {
        sqlx::query(
            "INSERT INTO capture_edges (judge_name, court_id, outcome_signal, observed_at, source_ref)
             VALUES ($1, $2, 0.0, $3, $4)",
        )
        .bind(&judge)
        .bind(format!("testcourt-{tag}"))
        .bind(NaiveDate::from_ymd_opt(2021, 1, 1).unwrap() + chrono::Days::new(i))
        .bind(format!("test:{tag}:{i}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    // The null-model floor is enforced.
    let (st, _) = send(
        app.clone(),
        "POST",
        "/capture/compute",
        Some(json!({ "permutations": 10, "seed": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    let (st, body) = send(
        app.clone(),
        "POST",
        "/capture/compute",
        Some(json!({ "permutations": 1000, "seed": 99 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let report = json_body(&body);
    assert_eq!(report["permutations"], 1000);
    assert_eq!(report["seed"], 99);
    assert!(report["judges"].as_u64().unwrap() >= 1);

    let (st, body) = send(app.clone(), "GET", "/capture/outliers?max_p=1.0", None).await;
    assert_eq!(st, StatusCode::OK);
    let outliers = json_body(&body)["outliers"].as_array().unwrap().clone();
    let mine = outliers
        .iter()
        .find(|o| o["entity_key"].as_str().unwrap_or("").contains(&judge));
    let mine = mine.expect("synthetic judge must be scored");
    assert_eq!(mine["entity_kind"], "judge");
    assert_eq!(mine["status"], "pending");
    assert_eq!(mine["appearances"], 6);
    assert!(mine["null_p"].as_f64().unwrap() <= 1.0);

    // Pipeline: a case whose opinion mentions a doctrine AND carries an
    // outcome-signal lexicon hit, so every meta stage has real input.
    let case_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO court_cases (case_id, docket_number, jurisdiction, court_level)
         VALUES ($1, $2, 'CA', 'superior')",
    )
    .bind(case_id)
    .bind(format!("META-{case_id}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO court_opinions
           (opinion_id, case_id, court_level, judge, citation, date_issued, full_text)
         VALUES ($1, $2, 'superior', 'Meta Test J.', 'Meta Test (2026)', '2026-02-01',
                 'The Brady material was withheld. The suppression motion is denied and the conviction is affirmed.')",
    )
    .bind(Uuid::new_v4())
    .bind(case_id)
    .execute(&pool)
    .await
    .unwrap();

    let (st, body) = send(
        app.clone(),
        "POST",
        "/pipeline/run",
        Some(json!({ "case_id": case_id })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let summary = json_body(&body);

    let run = &summary["runs"][0];
    let order: Vec<&str> = run["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["stage"].as_str().unwrap())
        .collect();
    let pos = |name: &str| order.iter().position(|s| *s == name);
    // Resonance is last, after every signal producer.
    assert_eq!(pos("resonance_compute"), Some(order.len() - 1));
    assert!(pos("drift_ingest_signals") < pos("drift_detect"));
    assert!(pos("capture_rebuild") < pos("capture_metrics"));
    assert!(pos("capture_metrics") < pos("resonance_compute"));

    // The case had everything these stages need: all processed, none skipped.
    let counts = &summary["stage_counts"];
    assert_eq!(counts["drift_ingest_signals"]["processed"], 1);
    assert_eq!(counts["capture_rebuild"]["processed"], 1);
    assert_eq!(counts["capture_metrics"]["processed"], 1);
    assert_eq!(counts["resonance_compute"]["processed"], 1);

    // The screen hit + lexicon match mean this case feeds a drift series.
    assert_eq!(counts["drift_detect"]["processed"], 1);
    let drift_stage = run["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["stage"] == "drift_detect")
        .unwrap();
    assert!(drift_stage["detail"]["series"].as_array().unwrap().len() >= 1);

    let report = ledger.verify().await.unwrap();
    assert!(report.ok, "ledger chain must verify: {report:?}");
}
