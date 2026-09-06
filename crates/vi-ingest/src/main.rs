//! The ingestion service: poll the configured public feeds, then walk every
//! new record through the engines.
//!
//! Run once (default) or on an interval with `INGEST_INTERVAL_SECS`.
//! Configuration lives in the environment and is logged on startup, so what a
//! deployment is reading from is never a mystery:
//!
//! ```text
//! INGEST_SOURCES        comma-separated feed names, or `all` (the default)
//! CL_SEARCH_QUERIES     semicolon-separated search queries
//! CL_FEED_COURTS        comma-separated court ids for Atom feeds
//! CL_API_TOKEN          enables the authenticated feed with complete text
//! CL_COURTS_PAGES       registry pages per cycle; a crawl that runs out
//!                       resumes where it stopped on the next cycle
//! INGEST_BACKFILL       follow cursors backwards instead of re-reading the head
//! INGEST_INTERVAL_SECS  loop interval; unset means a single cycle
//! INGEST_PIPELINE       set to 0 to ingest without running the engines
//! INGEST_PIPELINE_LIMIT cases per cycle handed to the pipeline (default 200)
//! INGEST_COURT_LOOKUP   set to 0 to never fetch a court the registry lacks
//! ```
use anyhow::Result;
use vi_ingest::{configured_from_env, CycleRequest};

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
    let interval = std::env::var("INGEST_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&s| s > 0);

    let mut req = CycleRequest::live();
    // Only the scheduler declares. A single cycle is an operator running one
    // feed by hand, and letting that redefine what the deployment reads would
    // leave the sources page describing a one-off command as the whole system.
    req.declare = interval.is_some();

    tracing::info!(
        feeds = ?specs.iter().map(|s| s.name()).collect::<Vec<_>>(),
        pipeline = req.pipeline,
        interval_secs = ?interval,
        "ingestion starting"
    );

    if req.declare {
        vi_ingest::declare_configured(&pool, &specs).await?;
        // Declared once at startup. Per-cycle declare would rewrite history
        // if a one-off env change landed mid-loop.
        req.declare = false;
    }

    let mut cycle = 0u64;
    loop {
        cycle += 1;
        match vi_ingest::run_cycle(&pool, &ledger, &req).await {
            Ok(report) => {
                tracing::info!(
                    cycle,
                    feeds = report.feeds.len(),
                    cases = report.totals.cases,
                    opinions = report.totals.opinions,
                    courts = report.totals.courts,
                    skipped = report.totals.skipped,
                    "ingest cycle complete"
                );
                if let Some(err) = &report.feed_error {
                    tracing::error!(cycle, error = %err, "every feed failed this cycle");
                }
                if let Some(courts) = &report.courts {
                    let updated = courts
                        .get("cases_updated")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    if updated > 0 {
                        tracing::info!(
                            cycle,
                            cases = updated,
                            "placed cases whose court was unknown"
                        );
                    }
                }
                if let Some(pipe) = &report.pipeline {
                    tracing::info!(
                        cycle,
                        cases = pipe.cases_processed,
                        ok = pipe.ok,
                        partial = pipe.partial,
                        screens = pipe.screens,
                        flags = pipe.flags_fired,
                        gaps = pipe.evidence_gaps,
                        actors = pipe.actors_linked,
                        "pipeline cycle complete (all artifacts pending review)"
                    );
                }
            }
            Err(e) => tracing::error!(cycle, error = %e, "ingest cycle failed"),
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
