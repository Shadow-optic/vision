use anyhow::Result;
use vi_ingest::{persist_case, CourtListenerClient, Source};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let pool = vi_db::pool_from_env().await?;
    vi_db::migrate(&pool).await?;
    let ledger = vi_ledger::Ledger::new(pool.clone());
    let client = CourtListenerClient::new(std::env::var("CL_API_TOKEN").ok());

    let interval = std::env::var("INGEST_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0);

    loop {
        match client.poll().await {
            Ok(records) => {
                tracing::info!(source = client.name(), n = records.len(), "polled");
                for r in &records {
                    match persist_case(&pool, &ledger, r).await {
                        Ok(_) => tracing::info!(docket = %r.docket_number, "persisted"),
                        Err(e) => {
                            tracing::error!(docket = %r.docket_number, error = %e, "persist failed")
                        }
                    }
                }
            }
            Err(e) => tracing::error!(error = %e, "poll failed"),
        }

        match interval {
            Some(secs) => tokio::time::sleep(std::time::Duration::from_secs(secs)).await,
            None => break,
        }
    }
    Ok(())
}
