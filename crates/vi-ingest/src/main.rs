//! The ingestion service: poll the configured public feeds, then walk every
//! new record through the engines.
//!
//! Run once (default) or on an interval with `INGEST_INTERVAL_SECS`.
//! Configuration lives in the environment and is logged on startup, so what a
//! deployment is reading from is never a mystery:
//!
//! ```text
//! INGEST_SOURCES        comma-separated feed names, or `all` (default: all)
//! CL_SEARCH_QUERIES     semicolon-separated search queries
//! CL_FEED_COURTS        comma-separated court ids for Atom feeds
//! CL_API_TOKEN          enables the authenticated feed with complete text
//! CL_COURTS_PAGES       registry pages per cycle
//! INGEST_BACKFILL       follow cursors backwards instead of re-reading the head
//! INGEST_INTERVAL_SECS  loop interval; unset means a single cycle
//! INGEST_PIPELINE       set to 0 to ingest without running the engines
//! INGEST_PIPELINE_LIMIT cases per cycle handed to the pipeline (default 200)
//! ```
use anyhow::Result;
use vi_ingest::{configured_from_env, run_specs};

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let pool = vi_db::pool_from_env().await?;
    vi_db::migrate(&pool).await?;
    let ledger = vi_ledger::Ledger::new(pool.clone());

    let specs = configured_from_env();
    if specs.is_empty() {
        anyhow::bail!(
            "no ingest feeds configured; set INGEST_SOURCES (e.g. \
             courtlistener-courts,courtlistener-search) or `all`"
        );
    }
    let run_pipeline = std::env::var("INGEST_PIPELINE").unwrap_or_default().trim() != "0";
    let pipeline_limit = env_usize("INGEST_PIPELINE_LIMIT", 200) as i64;
    let interval = std::env::var("INGEST_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&s| s > 0);

    tracing::info!(
        feeds = ?specs.iter().map(|s| s.name()).collect::<Vec<_>>(),
        pipeline = run_pipeline,
        interval_secs = ?interval,
        "ingestion starting"
    );

    let mut cycle = 0u64;
    loop {
        cycle += 1;
        match run_specs(&pool, &ledger, &specs).await {
            Ok(reports) => {
                let cases: u64 = reports.iter().map(|r| r.cases_persisted).sum();
                let opinions: u64 = reports.iter().map(|r| r.opinions_persisted).sum();
                let courts: u64 = reports.iter().map(|r| r.courts_persisted).sum();
                let skipped: u64 = reports.iter().map(|r| r.skipped).sum();
                tracing::info!(
                    cycle,
                    feeds = reports.len(),
                    cases,
                    opinions,
                    courts,
                    skipped,
                    "ingest cycle complete"
                );
            }
            Err(e) => tracing::error!(cycle, error = %e, "every feed failed this cycle"),
        }

        if run_pipeline {
            match vi_pipeline::run_pending(&pool, &ledger, pipeline_limit, "ingest").await {
                Ok(summary) => tracing::info!(
                    cycle,
                    cases = summary.cases_processed,
                    ok = summary.ok,
                    partial = summary.partial,
                    screens = summary.screens,
                    flags = summary.flags_fired,
                    gaps = summary.evidence_gaps,
                    actors = summary.actors_linked,
                    "pipeline cycle complete (all artifacts pending review)"
                ),
                Err(e) => tracing::error!(cycle, error = %e, "pipeline cycle failed"),
            }
        }

        let Some(secs) = interval else { break };
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(secs)) => {}
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown requested; stopping after this cycle");
                break;
            }
        }
    }
    Ok(())
}
