//! VisionInjustice public API. AuthN/Z for the attorney portal is Phase 4;
//! this service assumes it sits behind a gateway for anything non-public.
use axum::{
    routing::{get, post},
    Router,
};
use std::time::Duration;
use tokio::signal;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};
use vi_api::handlers::{self, AppState};
use vi_ledger::Ledger;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let pool = vi_db::pool_from_env().await?;
    vi_db::migrate(&pool).await?;
    match handlers::backfill_h3(&pool).await {
        Ok(n) => tracing::info!(cases = n, "h3 backfill complete"),
        Err(e) => tracing::warn!(error = %e, "h3 backfill skipped"),
    }

    let state = AppState {
        ledger: Ledger::new(pool.clone()),
        pool,
    };

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::ready))
        .route("/cases/search", get(handlers::search))
        .route("/cases/:id", get(handlers::case_context))
        .route("/prosecutors/:id/stats", get(handlers::prosecutor_stats))
        .route("/geo/cells/:cell", get(handlers::cell_stats))
        .route(
            "/rules",
            get(handlers::list_rules).post(handlers::create_rule),
        )
        .route("/rules/run", post(handlers::run_rules))
        .route("/flags", get(handlers::list_flags))
        .route("/simulate", post(handlers::simulate))
        .route("/ledger/verify", get(handlers::verify_ledger))
        .route("/stats/pearson", post(handlers::pearson))
        .route("/stats/odds", post(handlers::odds))
        .route("/stats/plea-sentence", get(handlers::plea_sentence_corr))
        .route("/lasm/package/:case_id", get(handlers::lasm_package))
        // Monell Atlas
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
        // Brady Reconciliation
        .route("/brady/derive/:case_id", post(handlers::brady_derive))
        .route("/brady/disclosed", post(handlers::brady_record_disclosed))
        .route("/brady/reconcile/:case_id", post(handlers::brady_reconcile))
        .route("/brady/lead-report/:case_id", get(handlers::brady_report))
        // Trial Penalty Observatory
        .route("/trial-penalty/offices", get(handlers::tp_office))
        .route("/trial-penalty/judges", get(handlers::tp_judge))
        .route("/trial-penalty/heatmap", get(handlers::tp_heatmap))
        .route("/trial-penalty/disparity", get(handlers::tp_disparity))
        .route("/trial-penalty/motion", get(handlers::tp_motion))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(60)))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("vi-api listening on {addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
