//! VisionInjustice public API. AuthN/Z for the attorney portal is Phase 4;
//! this service assumes it sits behind a gateway for anything non-public.
use std::time::Duration;
use tokio::signal;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};
use vi_api::handlers::AppState;
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
    match vi_api::handlers::backfill_h3(&pool).await {
        Ok(n) => tracing::info!(cases = n, "h3 backfill complete"),
        Err(e) => tracing::warn!(error = %e, "h3 backfill skipped"),
    }

    let state = AppState {
        ledger: Ledger::new(pool.clone()),
        pool,
    };

    let app = vi_api::router(state)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(60)))
        .layer(CorsLayer::permissive());

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
