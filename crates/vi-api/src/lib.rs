//! VisionInjustice HTTP API library. The `vi-api` binary is a thin wrapper.
#![forbid(unsafe_code)]

pub mod constitution;
pub mod error;
pub mod handlers;
pub mod reckoning;

use axum::{
    routing::{get, post},
    Router,
};
use handlers::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::ready))
        .route("/engines", get(handlers::engines))
        .route("/cases/search", get(handlers::search))
        .route("/cases/:id", get(handlers::case_context))
        .route("/prosecutors/:id/stats", get(handlers::prosecutor_stats))
        .route("/geo/cells/:cell", get(handlers::cell_stats))
        .route("/geo/kring/:cell", get(handlers::kring_stats))
        .route(
            "/rules",
            get(handlers::list_rules).post(handlers::create_rule),
        )
        .route("/rules/run", post(handlers::run_rules))
        .route("/flags", get(handlers::list_flags))
        .route("/simulate", post(handlers::simulate))
        .route(
            "/simulate/from-case/:case_id",
            post(handlers::simulate_from_case),
        )
        .route("/ledger/verify", get(handlers::verify_ledger))
        .route("/stats/pearson", post(handlers::pearson))
        .route("/stats/odds", post(handlers::odds))
        .route("/stats/plea-sentence", get(handlers::plea_sentence_corr))
        .route("/lasm/package/:case_id", get(handlers::lasm_package))
        .route(
            "/tactics",
            get(handlers::list_tactics).post(handlers::create_tactic),
        )
        .route("/tactics/:id", get(handlers::get_tactic))
        .route("/tactics/:id/stats", get(handlers::tactic_stats))
        .route("/ingest/run", post(handlers::ingest_run))
        .route("/ingest/status", get(handlers::ingest_status))
        .route("/atlas/findings", post(handlers::atlas_create_finding))
        .route(
            "/atlas/findings/:id/review",
            post(handlers::atlas_review_finding),
        )
        .route(
            "/atlas/offices/fingerprint",
            get(handlers::atlas_fingerprint),
        )
        .route("/atlas/offices/monell-report", get(handlers::atlas_report))
        .route("/brady/derive/:case_id", post(handlers::brady_derive))
        .route("/brady/disclosed", post(handlers::brady_record_disclosed))
        .route("/brady/reconcile/:case_id", post(handlers::brady_reconcile))
        .route("/brady/lead-report/:case_id", get(handlers::brady_report))
        .route("/trial-penalty/offices", get(handlers::tp_office))
        .route("/trial-penalty/judges", get(handlers::tp_judge))
        .route("/trial-penalty/heatmap", get(handlers::tp_heatmap))
        .route("/trial-penalty/disparity", get(handlers::tp_disparity))
        .route("/trial-penalty/motion", get(handlers::tp_motion))
        .route("/constitution", get(constitution::catalog))
        .route("/constitution/options", get(constitution::options))
        .route(
            "/constitution/jurisdictions",
            get(constitution::jurisdictions),
        )
        .route(
            "/constitution/provisions",
            get(constitution::list_provisions),
        )
        .route(
            "/constitution/provisions/:id",
            get(constitution::get_provision),
        )
        .route("/constitution/clauses", get(constitution::list_clauses))
        .route("/constitution/search", get(constitution::search))
        .route("/constitution/resolve", post(constitution::resolve))
        .route(
            "/constitution/screen/:case_id",
            get(constitution::screen_report).post(constitution::screen_run),
        )
        .route("/reckoning/actors", get(reckoning::list_actors))
        .route("/reckoning/actors/:id", get(reckoning::get_actor))
        .route(
            "/reckoning/actors/:id/score",
            get(reckoning::get_score).post(reckoning::persist_score),
        )
        .route(
            "/reckoning/actors/:id/package",
            post(reckoning::generate_package),
        )
        .route("/reckoning/actors/:id/publish", post(reckoning::publish))
        .route("/reckoning/resolve", post(reckoning::resolve))
        .route("/reckoning/sync", post(reckoning::sync))
        .route("/reckoning/packages", get(reckoning::list_packages))
        .route("/reckoning/packages/:id", get(reckoning::get_package))
        .route("/reckoning/wall", get(reckoning::wall))
        .route("/reckoning/wall/:id", get(reckoning::wall_profile))
        .route("/reckoning/tracker", get(reckoning::tracker))
        .route("/reckoning/statutes", get(reckoning::statute_catalog))
        .route("/reckoning/immunity", get(reckoning::immunity_catalog))
        .with_state(state)
}
