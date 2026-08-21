use anyhow::Result;
use vi_ingest::run_named;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let pool = vi_db::pool_from_env().await?;
    vi_db::migrate(&pool).await?;
    let ledger = vi_ledger::Ledger::new(pool.clone());

    let source = std::env::var("INGEST_SOURCE").unwrap_or_else(|_| {
        if std::env::var("CL_API_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some()
        {
            "courtlistener".into()
        } else {
            "fixture".into()
        }
    });

    let interval = std::env::var("INGEST_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0);

    loop {
        match run_named(&pool, &ledger, &source).await {
            Ok(report) => tracing::info!(
                source = %report.source,
                cases = report.cases_persisted,
                opinions = report.opinions_persisted,
                skipped = report.skipped,
                "ingest cycle complete"
            ),
            Err(e) => tracing::error!(error = %e, "ingest cycle failed"),
        }

        match interval {
            Some(secs) => tokio::time::sleep(std::time::Duration::from_secs(secs)).await,
            None => break,
        }
    }
    Ok(())
}
