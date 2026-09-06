//! Ingestion from public records.
//!
//! Every source implements [`Source`]. Records are normalized, placed in a
//! forum via the court registry, H3-tagged, hash-provenanced into the Root
//! Ledger, then upserted. Polling is idempotent: a feed re-read a thousand
//! times leaves the same rows behind.
//!
//! What ingestion refuses to do is as important as what it does. Sealed,
//! juvenile, expunged, and source-blocked records are dropped and counted
//! rather than stored. Nothing here concludes anything about a person; it
//! records what a public source published, with the provenance to prove it.
//!
//! PACER (fee-bearing, credentials), state portals, and exoneration-registry
//! imports plug in as additional `Source` impls — nothing downstream changes.
#![forbid(unsafe_code)]

pub mod atom;
pub mod courtlistener;
pub mod jurisdiction;

use anyhow::{bail, Result};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

/// How much of the source document the stored text represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Completeness {
    /// The whole document.
    Full,
    /// A search-result extract.
    Snippet,
    /// A feed summary.
    Summary,
}

impl Completeness {
    pub fn as_str(self) -> &'static str {
        match self {
            Completeness::Full => "full",
            Completeness::Snippet => "snippet",
            Completeness::Summary => "summary",
        }
    }

    pub fn is_partial(self) -> bool {
        !matches!(self, Completeness::Full)
    }
}

#[derive(Debug, Clone)]
pub struct NormalizedCase {
    pub docket_number: String,
    /// Best-known forum before the registry is consulted: a forum code when
    /// the source gives one, otherwise the source's court id.
    pub jurisdiction: String,
    /// The source's court id, used to look up the real forum.
    pub source_court_id: Option<String>,
    pub court_level: Option<String>,
    pub charge_category: Option<String>,
    /// Public-record judge name when the source prints one.
    pub judge: Option<String>,
    pub filed: Option<NaiveDate>,
    pub court_lat: Option<f64>,
    pub court_lng: Option<f64>,
    pub source_url: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone)]
pub struct NormalizedOpinion {
    pub docket_number: Option<String>,
    pub citation: Option<String>,
    pub court_level: Option<String>,
    pub judge: Option<String>,
    pub date_issued: Option<NaiveDate>,
    pub text: String,
    pub completeness: Completeness,
    pub source_url: Option<String>,
    /// Stable id at the source; also the idempotence key.
    pub source_ref: Option<String>,
}

/// One court as published by a courts feed.
#[derive(Debug, Clone)]
pub struct CourtRecord {
    pub court_id: String,
    pub full_name: String,
    pub short_name: Option<String>,
    pub citation_string: Option<String>,
    pub source_class: Option<String>,
    pub in_use: bool,
    pub parent_court: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub source_url: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, Default)]
pub struct PollResult {
    pub cases: Vec<NormalizedCase>,
    pub opinions: Vec<NormalizedOpinion>,
    pub courts: Vec<CourtRecord>,
    /// Records the source offered and the source itself declined to normalize.
    pub skipped: u64,
    pub next_cursor: Option<String>,
    /// Set when the poll stopped before the end of the feed but kept what it
    /// read and a cursor to resume from. A long list interrupted by a rate
    /// limit is progress, not an error, and saying so is the difference
    /// between a feed that finishes eventually and one that restarts forever.
    pub paused: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub source: String,
    pub kind: String,
    pub label: String,
    /// Records written this poll, including records the feed had shown before.
    pub cases_persisted: u64,
    pub opinions_persisted: u64,
    pub courts_persisted: u64,
    /// Records that did not exist until this poll. A live feed re-reads its
    /// head constantly, so this is the number that means something.
    pub cases_new: u64,
    pub opinions_new: u64,
    pub skipped: u64,
    pub next_cursor: Option<String>,
    /// Why the poll stopped short of the end of the feed, if it did.
    pub paused: Option<String>,
}

pub trait Source {
    /// Stable identity, used as the cursor key.
    fn name(&self) -> &str;
    /// Feed family: `registry`, `search`, `atom`, `api`, or `fixture`.
    fn kind(&self) -> &'static str;
    /// Human-readable description, published on the sources page.
    fn label(&self) -> String;
    #[allow(async_fn_in_trait)]
    async fn poll(&self, cursor: Option<&str>) -> Result<PollResult>;
}

/// Sealed, juvenile, expunged, or source-blocked records are excluded.
pub fn is_excluded(docket: &str, raw: &Value) -> bool {
    if raw.get("blocked").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    let blob = format!("{docket} {raw}").to_ascii_lowercase();
    blob.contains("sealed") || blob.contains("juvenile") || blob.contains("expunged")
}

/// Short stable key for a string, for synthesising identifiers.
pub fn hash_key(input: &str) -> String {
    vi_ledger::hash_payload(&json!(input))
        .chars()
        .take(16)
        .collect()
}

// ===== Feed configuration =====

/// A configured feed. Configuration decides which slice of the public record
/// is looked at first; it decides nothing about what is concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedSpec {
    Fixture,
    Courts { pages: usize },
    Search { query: String, backfill: bool },
    Atom { court: String },
    Api,
}

impl FeedSpec {
    pub fn name(&self) -> String {
        match self {
            FeedSpec::Fixture => "fixture".into(),
            FeedSpec::Courts { .. } => "courtlistener-courts".into(),
            FeedSpec::Search { query, .. } => {
                format!("courtlistener-search/{}", courtlistener::slug(query))
            }
            FeedSpec::Atom { court } => format!("courtlistener-feed/{court}"),
            FeedSpec::Api => "courtlistener".into(),
        }
    }

    pub fn build(&self) -> Result<Feed> {
        Ok(match self {
            FeedSpec::Fixture => Feed::Fixture(FixtureSource),
            FeedSpec::Courts { pages } => Feed::Courts(courtlistener::CourtRegistry::new(*pages)?),
            FeedSpec::Search { query, backfill } => {
                Feed::Search(courtlistener::SearchFeed::new(query, *backfill)?)
            }
            FeedSpec::Atom { court } => Feed::Atom(courtlistener::CourtFeed::new(court)?),
            FeedSpec::Api => Feed::Api(courtlistener::ApiClient::new(
                std::env::var("CL_API_TOKEN").unwrap_or_default(),
            )?),
        })
    }
}

/// Built feeds, so a cycle can hold heterogeneous sources.
pub enum Feed {
    Fixture(FixtureSource),
    Courts(courtlistener::CourtRegistry),
    Search(courtlistener::SearchFeed),
    Atom(courtlistener::CourtFeed),
    Api(courtlistener::ApiClient),
}

macro_rules! dispatch {
    ($self:ident, $method:ident $(, $arg:expr)*) => {
        match $self {
            Feed::Fixture(s) => s.$method($($arg),*),
            Feed::Courts(s) => s.$method($($arg),*),
            Feed::Search(s) => s.$method($($arg),*),
            Feed::Atom(s) => s.$method($($arg),*),
            Feed::Api(s) => s.$method($($arg),*),
        }
    };
}

impl Source for Feed {
    fn name(&self) -> &str {
        dispatch!(self, name)
    }

    fn kind(&self) -> &'static str {
        dispatch!(self, kind)
    }

    fn label(&self) -> String {
        dispatch!(self, label)
    }

    async fn poll(&self, cursor: Option<&str>) -> Result<PollResult> {
        match self {
            Feed::Fixture(s) => s.poll(cursor).await,
            Feed::Courts(s) => s.poll(cursor).await,
            Feed::Search(s) => s.poll(cursor).await,
            Feed::Atom(s) => s.poll(cursor).await,
            Feed::Api(s) => s.poll(cursor).await,
        }
    }
}

fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).unwrap_or_default().trim(),
        "1" | "true" | "yes" | "on"
    )
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Queries that decide where to look first. Accountability-relevant language
/// that appears in published opinions, not accusations against anyone.
pub const DEFAULT_SEARCH_QUERIES: &[&str] = &[
    "brady violation",
    "prosecutorial misconduct",
    "fabricated evidence",
];

pub const DEFAULT_FEED_COURTS: &[&str] = &["ca9", "scotus"];

/// Feeds configured for this deployment.
///
/// `INGEST_SOURCES` is a comma-separated list of feed names, or `all`.
/// Search queries come from `CL_SEARCH_QUERIES` (semicolon-separated, since
/// queries contain commas), courts for Atom feeds from `CL_FEED_COURTS`.
pub fn configured_from_env() -> Vec<FeedSpec> {
    sources_from(
        env_nonempty("INGEST_SOURCES")
            .or_else(|| env_nonempty("INGEST_SOURCE"))
            .as_deref(),
    )
}

/// Resolve a requested feed list, or the defaults when nothing was requested.
/// Takes the value rather than reading it so that `INGEST_SOURCES=all` can be
/// exercised without a deployment to set it in.
fn sources_from(requested: Option<&str>) -> Vec<FeedSpec> {
    match requested {
        Some(requested) => expand_list(requested),
        None => expand_defaults(),
    }
}

/// The feeds read when a deployment has not named any. The authenticated feed
/// is included only when there is a token for it, since polling it without one
/// yields nothing but refusals.
fn default_source_tokens() -> &'static [&'static str] {
    if env_nonempty("CL_API_TOKEN").is_some() {
        &[
            "courtlistener-courts",
            "courtlistener",
            "courtlistener-search",
        ]
    } else {
        &[
            "courtlistener-courts",
            "courtlistener-search",
            "courtlistener-feed",
        ]
    }
}

/// Expand the default feeds. Kept separate from [`configured_from_env`] so
/// that `all` resolves to the default *tokens* rather than to whatever
/// `INGEST_SOURCES` says — which, when it said `all`, was itself.
fn expand_defaults() -> Vec<FeedSpec> {
    let mut specs = Vec::new();
    for token in default_source_tokens() {
        specs.extend(expand(token));
    }
    dedup(specs)
}

/// Expand a comma-separated list of feed names.
fn expand_list(requested: &str) -> Vec<FeedSpec> {
    let mut specs = Vec::new();
    for token in requested.split(',') {
        specs.extend(expand(token.trim()));
    }
    dedup(specs)
}

/// One entry per feed. `Vec::dedup` drops only neighbours, which left
/// `all,courtlistener-courts` polling the court list twice a cycle.
fn dedup(specs: Vec<FeedSpec>) -> Vec<FeedSpec> {
    let mut seen = std::collections::HashSet::new();
    specs
        .into_iter()
        .filter(|s| seen.insert(s.name()))
        .collect()
}

fn search_queries() -> Vec<String> {
    match env_nonempty("CL_SEARCH_QUERIES") {
        Some(raw) => raw
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        None => DEFAULT_SEARCH_QUERIES
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

fn feed_courts() -> Vec<String> {
    match env_nonempty("CL_FEED_COURTS") {
        Some(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        None => DEFAULT_FEED_COURTS.iter().map(|s| s.to_string()).collect(),
    }
}

fn courts_pages() -> usize {
    env_nonempty("CL_COURTS_PAGES")
        .and_then(|s| s.parse().ok())
        .unwrap_or(4)
}

/// Expand one feed name into concrete specs. Unknown names expand to nothing;
/// [`run_named`] reports them as errors rather than silently doing nothing.
///
/// No token here resolves through `INGEST_SOURCES`. `all` once meant "whatever
/// that variable says", so setting it to `all` — the value the documentation
/// suggests — asked the variable what it meant and crashed the service on a
/// stack overflow before it read a single record. `configured` is resolved by
/// [`run_named`] instead, where the request comes from outside the config.
pub fn expand(token: &str) -> Vec<FeedSpec> {
    let backfill = env_flag("INGEST_BACKFILL");
    match token {
        "" => vec![],
        "all" => expand_defaults(),
        "fixture" => vec![FeedSpec::Fixture],
        "courts" | "courtlistener-courts" => vec![FeedSpec::Courts {
            pages: courts_pages(),
        }],
        "courtlistener" | "courtlistener-api" => vec![FeedSpec::Api],
        "courtlistener-search" => search_queries()
            .into_iter()
            .map(|query| FeedSpec::Search { query, backfill })
            .collect(),
        "courtlistener-feed" => feed_courts()
            .into_iter()
            .map(|court| FeedSpec::Atom { court })
            .collect(),
        other => {
            if let Some(rest) = other.strip_prefix("courtlistener-search/") {
                // Either a configured query's slug, or a query spelled out.
                let query = search_queries()
                    .into_iter()
                    .find(|q| courtlistener::slug(q) == rest)
                    .unwrap_or_else(|| rest.replace('-', " "));
                return vec![FeedSpec::Search { query, backfill }];
            }
            if let Some(court) = other.strip_prefix("courtlistener-feed/") {
                return vec![FeedSpec::Atom {
                    court: court.to_string(),
                }];
            }
            vec![]
        }
    }
}

// ===== Cursors and status =====

async fn load_cursor(pool: &PgPool, source: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT next_url FROM ingest_cursors WHERE source = $1",
    )
    .bind(source)
    .fetch_optional(pool)
    .await?
    .flatten())
}

async fn mark_started(pool: &PgPool, source: &Feed) -> Result<()> {
    sqlx::query(
        "INSERT INTO ingest_cursors (source, feed_kind, label, last_started_at)
         VALUES ($1,$2,$3,now())
         ON CONFLICT (source) DO UPDATE SET
            feed_kind = EXCLUDED.feed_kind,
            label = EXCLUDED.label,
            last_started_at = EXCLUDED.last_started_at",
    )
    .bind(source.name())
    .bind(source.kind())
    .bind(source.label())
    .execute(pool)
    .await?;
    Ok(())
}

/// What one poll produced.
struct Counts {
    cases: i32,
    opinions: i32,
    cases_new: i32,
    opinions_new: i32,
    skipped: i32,
}

/// Record a poll that kept what it read. `paused` carries the reason the poll
/// stopped short of the end of the feed, and only a poll that reached the end
/// advances `last_ok_at` — otherwise a crawl stalled on page 23 forever would
/// report a fresh clean pass every cycle.
async fn save_success(
    pool: &PgPool,
    source: &str,
    next: Option<&str>,
    counts: Counts,
    paused: Option<&str>,
) -> Result<()> {
    // Running totals count new records only; a feed polled hourly forever
    // would otherwise report a number that grows without anything happening.
    sqlx::query(
        "UPDATE ingest_cursors SET
            next_url = $2,
            last_polled_at = now(),
            last_ok_at = CASE WHEN $8::text IS NULL THEN now() ELSE last_ok_at END,
            last_error = NULL,
            last_pause = $8,
            consecutive_failures = 0,
            last_count = $3 + $4,
            last_cases = $3,
            last_opinions = $4,
            last_skipped = $7,
            total_cases = total_cases + $5,
            total_opinions = total_opinions + $6,
            total_skipped = total_skipped + $7
          WHERE source = $1",
    )
    .bind(source)
    .bind(next)
    .bind(counts.cases)
    .bind(counts.opinions)
    .bind(counts.cases_new)
    .bind(counts.opinions_new)
    .bind(counts.skipped)
    .bind(paused)
    .execute(pool)
    .await?;
    Ok(())
}

async fn save_failure(pool: &PgPool, source: &str, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE ingest_cursors SET
            last_polled_at = now(),
            last_error = $2,
            last_pause = NULL,
            consecutive_failures = consecutive_failures + 1,
            last_count = 0, last_cases = 0, last_opinions = 0, last_skipped = 0
          WHERE source = $1",
    )
    .bind(source)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record the feeds this service polls, so that anything reporting on coverage
/// reads the list from the process doing the polling.
///
/// The API answered from its own environment, and when the two services were
/// configured separately it published a live feed as "not configured" — a
/// coverage gap that did not exist, over records that did.
pub async fn declare_configured(pool: &PgPool, specs: &[FeedSpec]) -> Result<()> {
    let names: Vec<String> = specs.iter().map(FeedSpec::name).collect();
    for spec in specs {
        let feed = spec.build();
        sqlx::query(
            "INSERT INTO ingest_cursors (source, feed_kind, label, configured)
             VALUES ($1,$2,$3,TRUE)
             ON CONFLICT (source) DO UPDATE SET
                configured = TRUE,
                feed_kind = COALESCE(EXCLUDED.feed_kind, ingest_cursors.feed_kind),
                label = COALESCE(EXCLUDED.label, ingest_cursors.label)",
        )
        .bind(spec.name())
        .bind(feed.as_ref().ok().map(|f| f.kind()))
        .bind(feed.as_ref().ok().map(|f| f.label()))
        .execute(pool)
        .await?;
    }
    // A feed dropped from the configuration keeps its history and stops
    // claiming to be read.
    sqlx::query("UPDATE ingest_cursors SET configured = FALSE WHERE source <> ALL($1)")
        .bind(&names)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_cursors(pool: &PgPool) -> Result<Vec<Value>> {
    let rows = sqlx::query_scalar::<_, Value>(
        r#"SELECT jsonb_build_object(
             'source', source, 'feed_kind', feed_kind, 'label', label,
             'next_url', next_url, 'configured', configured,
             'last_polled_at', last_polled_at, 'last_ok_at', last_ok_at,
             'last_count', last_count, 'last_cases', last_cases,
             'last_opinions', last_opinions, 'last_skipped', last_skipped,
             'new_cases', total_cases, 'new_opinions', total_opinions,
             'total_skipped', total_skipped,
             'consecutive_failures', consecutive_failures,
             'last_error', last_error, 'last_pause', last_pause)
           FROM ingest_cursors ORDER BY source"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Every feed with what the database knows about it, so a feed that is
/// configured but has never polled is visible as exactly that.
///
/// Whether a feed is polled comes from the ingestion service's own declaration
/// where it has made one. This process's environment is only a fallback: the
/// API and the ingestion service can be configured separately, and answering
/// from the wrong one published live feeds as switched off.
pub async fn list_sources(pool: &PgPool) -> Result<Value> {
    let local = configured_from_env();
    let rows = list_cursors(pool).await?;
    let declared = |name: &str| -> Option<bool> {
        rows.iter()
            .find(|r| r.get("source").and_then(Value::as_str) == Some(name))
            .and_then(|r| r.get("configured").and_then(Value::as_bool))
    };

    let mut sources = Vec::new();
    for spec in &local {
        let name = spec.name();
        let status = rows
            .iter()
            .find(|r| r.get("source").and_then(Value::as_str) == Some(name.as_str()))
            .cloned();
        let label = spec
            .build()
            .map(|f| f.label())
            .unwrap_or_else(|e| format!("unavailable: {e}"));
        sources.push(json!({
            "source": name,
            "label": label,
            "configured": declared(&name).unwrap_or(true),
            "status": status,
        }));
    }
    // Feeds the poller reads that this process was not told about, and feeds
    // that ran under a previous configuration and still have history.
    for row in &rows {
        let name = row.get("source").and_then(Value::as_str).unwrap_or("");
        if local.iter().any(|s| s.name() == name) {
            continue;
        }
        sources.push(json!({
            "source": name,
            "label": row.get("label").cloned().unwrap_or(Value::Null),
            "configured": declared(name).unwrap_or(false),
            "status": row.clone(),
        }));
    }

    let (case_count, opinion_count, partial, courts): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM court_cases),
                (SELECT COUNT(*) FROM court_opinions),
                (SELECT COUNT(*) FROM court_opinions WHERE text_completeness <> 'full'),
                (SELECT COUNT(*) FROM court_registry)",
    )
    .fetch_one(pool)
    .await?;

    Ok(json!({
        "sources": sources,
        "totals": {
            "cases": case_count,
            "opinions": opinion_count,
            "partial_text_opinions": partial,
            "courts_registered": courts,
        },
        "exclusions": [
            "sealed", "juvenile", "expunged", "source-blocked", "empty text"
        ],
        "note": "Ingestion records what a public source published. It concludes nothing about any individual.",
    }))
}

// ===== Execution =====

pub async fn execute(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    source: &Feed,
) -> Result<RunReport> {
    let name = source.name().to_string();
    mark_started(pool, source).await?;
    let cursor = load_cursor(pool, &name).await?;

    let poll = match source.poll(cursor.as_deref()).await {
        Ok(poll) => poll,
        Err(e) => {
            save_failure(pool, &name, &e.to_string()).await?;
            return Err(e);
        }
    };

    let mut skipped = poll.skipped;
    let mut cases_n = 0u64;
    let mut cases_new = 0u64;
    let mut opinions_n = 0u64;
    let mut opinions_new = 0u64;
    let mut courts_n = 0u64;

    for court in &poll.courts {
        persist_court(pool, court).await?;
        courts_n += 1;
    }

    for c in &poll.cases {
        if is_excluded(&c.docket_number, &c.raw) {
            skipped += 1;
            continue;
        }
        let (_, inserted) = persist_case(pool, ledger, c).await?;
        cases_n += 1;
        if inserted {
            cases_new += 1;
        }
    }

    for o in &poll.opinions {
        if o.text.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let Some(docket) = o.docket_number.as_deref() else {
            skipped += 1;
            continue;
        };
        if is_excluded(docket, &json!({"citation": o.citation})) {
            skipped += 1;
            continue;
        }
        let case_id = match lookup_case_id(pool, docket).await? {
            Some(id) => id,
            None => {
                let (id, inserted) = persist_case(
                    pool,
                    ledger,
                    &NormalizedCase {
                        docket_number: docket.to_string(),
                        jurisdiction: "unknown".into(),
                        source_court_id: None,
                        court_level: o.court_level.clone(),
                        charge_category: None,
                        judge: o.judge.clone(),
                        filed: o.date_issued,
                        court_lat: None,
                        court_lng: None,
                        source_url: o.source_url.clone(),
                        raw: json!({"from": "opinion", "docket_number": docket}),
                    },
                )
                .await?;
                if inserted {
                    cases_new += 1;
                }
                id
            }
        };
        let (_, inserted) = persist_opinion(pool, ledger, case_id, o).await?;
        opinions_n += 1;
        if inserted {
            opinions_new += 1;
        }
    }

    save_success(
        pool,
        &name,
        poll.next_cursor.as_deref(),
        Counts {
            cases: cases_n as i32,
            opinions: opinions_n as i32,
            cases_new: cases_new as i32,
            opinions_new: opinions_new as i32,
            skipped: skipped as i32,
        },
        poll.paused.as_deref(),
    )
    .await?;

    Ok(RunReport {
        source: name,
        kind: source.kind().to_string(),
        label: source.label(),
        cases_persisted: cases_n,
        opinions_persisted: opinions_n,
        courts_persisted: courts_n,
        cases_new,
        opinions_new,
        skipped,
        next_cursor: poll.next_cursor,
        paused: poll.paused,
    })
}

/// Run one feed name, a family of feeds, `all`, or `configured`.
///
/// `configured` is this deployment's own `INGEST_SOURCES` list, resolved here
/// rather than in [`expand`] so that a feed name can never resolve through the
/// configuration that named it.
pub async fn run_named(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    source: &str,
) -> Result<Vec<RunReport>> {
    let source = source.trim();
    let specs = if source == "configured" {
        configured_from_env()
    } else {
        expand(source)
    };
    if specs.is_empty() {
        bail!(
            "unknown ingest source '{source}' (expected fixture, courts, courtlistener, \
             courtlistener-search[/query], courtlistener-feed[/court], all, or configured)"
        );
    }
    run_specs(pool, ledger, &specs).await
}

/// Run several feeds in order, reporting per feed. One failing feed does not
/// abort the cycle: a court that goes offline must not stop every other court.
pub async fn run_specs(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    specs: &[FeedSpec],
) -> Result<Vec<RunReport>> {
    let mut reports = Vec::new();
    let mut last_error = None;
    for spec in specs {
        let feed = match spec.build() {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(source = %spec.name(), error = %e, "feed unavailable");
                last_error = Some(e);
                continue;
            }
        };
        match execute(pool, ledger, &feed).await {
            Ok(report) => {
                tracing::info!(
                    source = %report.source,
                    cases = report.cases_persisted,
                    opinions = report.opinions_persisted,
                    courts = report.courts_persisted,
                    skipped = report.skipped,
                    "feed polled"
                );
                if let Some(why) = &report.paused {
                    tracing::info!(source = %report.source, reason = %why, "feed stopped short");
                }
                reports.push(report);
            }
            Err(e) => {
                tracing::error!(source = %spec.name(), error = %e, "feed failed");
                last_error = Some(e);
            }
        }
    }
    if reports.is_empty() {
        if let Some(e) = last_error {
            return Err(e);
        }
    }
    Ok(reports)
}

async fn lookup_case_id(pool: &PgPool, docket: &str) -> Result<Option<Uuid>> {
    Ok(
        sqlx::query_scalar::<_, Uuid>("SELECT case_id FROM court_cases WHERE docket_number = $1")
            .bind(docket)
            .fetch_optional(pool)
            .await?,
    )
}

// ===== Fixture source (CI / local, no network) =====

pub struct FixtureSource;

impl Source for FixtureSource {
    fn name(&self) -> &str {
        "fixture"
    }

    fn kind(&self) -> &'static str {
        "fixture"
    }

    fn label(&self) -> String {
        "Built-in fixture record (no network)".into()
    }

    async fn poll(&self, _cursor: Option<&str>) -> Result<PollResult> {
        Ok(fixture_poll())
    }
}

pub fn fixture_poll() -> PollResult {
    let docket = "FIXTURE-2024-001";
    let case = NormalizedCase {
        docket_number: docket.into(),
        jurisdiction: "CA".into(),
        source_court_id: Some("cand".into()),
        court_level: Some("superior".into()),
        charge_category: Some("drug".into()),
        judge: Some("Fixture J.".into()),
        filed: NaiveDate::from_ymd_opt(2024, 6, 1),
        court_lat: Some(37.7849),
        court_lng: Some(-122.4094),
        source_url: Some("https://example.test/fixture/FIXTURE-2024-001".into()),
        raw: json!({
            "docket_number": docket,
            "court_id": "cand",
            "date_filed": "2024-06-01",
        }),
    };
    let opinion = NormalizedOpinion {
        docket_number: Some(docket.into()),
        citation: Some("Fixture v. Demo (2024)".into()),
        court_level: Some("superior".into()),
        judge: Some("Fixture J.".into()),
        date_issued: NaiveDate::from_ymd_opt(2024, 8, 1),
        text: "The court referenced body-worn camera footage and a laboratory report. \
               A 911 call recording was discussed. Chain of custody was not produced."
            .into(),
        completeness: Completeness::Full,
        source_url: Some("https://example.test/fixture/opinion".into()),
        source_ref: Some("fixture:opinion:1".into()),
    };
    PollResult {
        cases: vec![case],
        opinions: vec![opinion],
        ..Default::default()
    }
}

// ===== Persistence =====

#[derive(Debug, sqlx::FromRow)]
struct UpsertRow {
    case_id: Uuid,
    inserted: bool,
}

/// Upsert one court and derive its forum.
pub async fn persist_court(pool: &PgPool, c: &CourtRecord) -> Result<()> {
    let mapping = jurisdiction::derive(
        &c.court_id,
        c.source_class.as_deref(),
        &c.full_name,
        c.citation_string.as_deref(),
    );
    sqlx::query(
        "INSERT INTO court_registry
           (court_id, full_name, short_name, citation_string, source_class,
            jurisdiction, court_level, mapping_method, in_use, parent_court,
            start_date, end_date, source_url, raw, refreshed_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now())
         ON CONFLICT (court_id) DO UPDATE SET
            full_name = EXCLUDED.full_name,
            short_name = EXCLUDED.short_name,
            citation_string = EXCLUDED.citation_string,
            source_class = EXCLUDED.source_class,
            jurisdiction = EXCLUDED.jurisdiction,
            court_level = EXCLUDED.court_level,
            mapping_method = EXCLUDED.mapping_method,
            in_use = EXCLUDED.in_use,
            parent_court = EXCLUDED.parent_court,
            start_date = EXCLUDED.start_date,
            end_date = EXCLUDED.end_date,
            source_url = EXCLUDED.source_url,
            raw = EXCLUDED.raw,
            refreshed_at = now()",
    )
    .bind(&c.court_id)
    .bind(&c.full_name)
    .bind(&c.short_name)
    .bind(&c.citation_string)
    .bind(&c.source_class)
    .bind(&mapping.jurisdiction)
    .bind(&mapping.court_level)
    .bind(mapping.method)
    .bind(c.in_use)
    .bind(&c.parent_court)
    .bind(c.start_date)
    .bind(c.end_date)
    .bind(&c.source_url)
    .bind(&c.raw)
    .execute(pool)
    .await?;
    Ok(())
}

/// The forum for a case: the registry first, then a static derivation.
///
/// A court that cannot be placed yields `unknown`, never the source's court id.
/// Writing "txctapp6" into a column named `jurisdiction` would state that a
/// case sits in a forum that does not exist, and screening reads that column to
/// decide which body of law applies.
async fn resolve_forum(
    pool: &PgPool,
    c: &NormalizedCase,
) -> Result<(String, Option<String>, &'static str)> {
    if let Some(court_id) = c.source_court_id.as_deref() {
        let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT jurisdiction, court_level, mapping_method FROM court_registry WHERE court_id = $1",
        )
        .bind(court_id)
        .fetch_optional(pool)
        .await?;
        if let Some((Some(jur), level, _)) = row {
            return Ok((jur, level.or_else(|| c.court_level.clone()), "registry"));
        }
        let derived = jurisdiction::derive(court_id, None, "", None);
        if let Some(jur) = derived.jurisdiction {
            return Ok((
                jur,
                derived.court_level.or_else(|| c.court_level.clone()),
                "court_id",
            ));
        }
    }
    // The source's own value is only a forum if it is one. A court id is not.
    let claimed = c.jurisdiction.trim();
    if vi_constitution::jurisdictions::lookup(claimed).is_some() {
        return Ok((claimed.to_string(), c.court_level.clone(), "source"));
    }

    // Nothing on hand places this court, so ask the source about it. This is
    // the last resort by design: it costs a request, and it only ever runs for
    // a court id the registry has never seen.
    if let Some(court_id) = c.source_court_id.as_deref() {
        if court_lookup_enabled() {
            match courtlistener::fetch_court(court_id).await {
                Ok(Some(record)) => {
                    persist_court(pool, &record).await?;
                    let mapping = jurisdiction::derive(
                        &record.court_id,
                        record.source_class.as_deref(),
                        &record.full_name,
                        record.citation_string.as_deref(),
                    );
                    if let Some(jur) = mapping.jurisdiction {
                        return Ok((
                            jur,
                            mapping.court_level.or_else(|| c.court_level.clone()),
                            "court_lookup",
                        ));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(court_id, error = %e, "could not look up a court");
                }
            }
        }
    }

    Ok(("unknown".to_string(), c.court_level.clone(), "unplaced"))
}

/// Place cases whose court was unknown when they were ingested.
///
/// A case sitting at `jurisdiction = 'unknown'` is a case no engine will screen,
/// because screening it would mean picking a body of law at random. One lookup
/// per distinct unplaced court repairs every case behind it.
pub async fn backfill_unplaced_courts(pool: &PgPool) -> Result<Value> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT source_court_id FROM court_cases
          WHERE jurisdiction = 'unknown' AND source_court_id IS NOT NULL
          ORDER BY source_court_id",
    )
    .fetch_all(pool)
    .await?;

    let mut placed = Vec::new();
    let mut unplaced = Vec::new();
    let mut cases_updated = 0u64;

    for court_id in ids {
        let mapping = match sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT jurisdiction, court_level FROM court_registry WHERE court_id = $1",
        )
        .bind(&court_id)
        .fetch_optional(pool)
        .await?
        {
            Some((Some(jur), level)) => Some((jur, level, "registry")),
            _ if court_lookup_enabled() => match courtlistener::fetch_court(&court_id).await {
                Ok(Some(record)) => {
                    persist_court(pool, &record).await?;
                    let m = jurisdiction::derive(
                        &record.court_id,
                        record.source_class.as_deref(),
                        &record.full_name,
                        record.citation_string.as_deref(),
                    );
                    m.jurisdiction
                        .map(|jur| (jur, m.court_level, "court_lookup"))
                }
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!(court_id, error = %e, "could not look up a court");
                    None
                }
            },
            _ => None,
        };

        match mapping {
            Some((jur, level, method)) => {
                let n = sqlx::query(
                    "UPDATE court_cases
                        SET jurisdiction = $2,
                            court_level = COALESCE(court_level, $3)
                      WHERE source_court_id = $1 AND jurisdiction = 'unknown'",
                )
                .bind(&court_id)
                .bind(&jur)
                .bind(&level)
                .execute(pool)
                .await?
                .rows_affected();
                cases_updated += n;
                placed.push(json!({
                    "court_id": court_id,
                    "jurisdiction": jur,
                    "court_level": level,
                    "method": method,
                    "cases": n,
                }));
            }
            None => unplaced.push(court_id),
        }
    }

    Ok(json!({
        "placed": placed,
        "still_unplaced": unplaced,
        "cases_updated": cases_updated,
        "note": "A case that cannot be placed in a forum is left unscreened rather \
                 than screened under a body of law that may not govern it.",
    }))
}

/// On-demand court lookups need the network. Off by default in tests and any
/// deployment that must ingest without reaching out.
fn court_lookup_enabled() -> bool {
    !matches!(
        std::env::var("INGEST_COURT_LOOKUP")
            .unwrap_or_default()
            .trim(),
        "0" | "false" | "no" | "off"
    )
}

/// Upsert + forum resolution + H3 ladder + ledger provenance for one record.
/// Returns the case id and whether this poll is what created it.
pub async fn persist_case(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    c: &NormalizedCase,
) -> Result<(Uuid, bool)> {
    let (court_h3, incident_h3) = match (c.court_lat, c.court_lng) {
        (Some(lat), Some(lng)) => (vi_geo::cell_for(lat, lng, 8).ok(), None::<String>),
        _ => (None, None),
    };
    let raw_hash = vi_ledger::hash_payload(&c.raw);
    let filed = c.filed.map(|d| {
        Utc.from_utc_datetime(&d.and_time(NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")))
    });
    let (jurisdiction, court_level, forum_method) = resolve_forum(pool, c).await?;

    let row = sqlx::query_as::<_, UpsertRow>(
        "INSERT INTO court_cases
           (case_id, docket_number, jurisdiction, court_level, charge_category, judge,
            filing_date, court_location_lat, court_location_lng,
            court_h3_cell, incident_h3_cell, source_url, raw_data, hash,
            source_court_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
         ON CONFLICT (docket_number) DO UPDATE SET
            jurisdiction = EXCLUDED.jurisdiction,
            source_court_id = COALESCE(EXCLUDED.source_court_id,
                                       court_cases.source_court_id),
            court_level = COALESCE(court_cases.court_level, EXCLUDED.court_level),
            charge_category = COALESCE(court_cases.charge_category, EXCLUDED.charge_category),
            judge = COALESCE(court_cases.judge, EXCLUDED.judge),
            filing_date = COALESCE(court_cases.filing_date, EXCLUDED.filing_date),
            source_url = COALESCE(EXCLUDED.source_url, court_cases.source_url),
            raw_data   = EXCLUDED.raw_data,
            hash       = EXCLUDED.hash,
            court_h3_cell = COALESCE(EXCLUDED.court_h3_cell, court_cases.court_h3_cell)
         RETURNING case_id, (xmax = 0) AS inserted",
    )
    .bind(Uuid::new_v4())
    .bind(&c.docket_number)
    .bind(&jurisdiction)
    .bind(&court_level)
    .bind(&c.charge_category)
    .bind(&c.judge)
    .bind(filed)
    .bind(c.court_lat)
    .bind(c.court_lng)
    .bind(&court_h3)
    .bind(&incident_h3)
    .bind(&c.source_url)
    .bind(&c.raw)
    .bind(&raw_hash)
    .bind(&c.source_court_id)
    .fetch_one(pool)
    .await?;

    let case_id = row.case_id;

    if let (Some(lat), Some(lng)) = (c.court_lat, c.court_lng) {
        for (res, cell) in vi_geo::ladder(lat, lng) {
            sqlx::query(
                "INSERT INTO case_h3_cells (case_id, h3_cell, resolution, cell_type)
                 VALUES ($1,$2,$3,'court') ON CONFLICT DO NOTHING",
            )
            .bind(case_id)
            .bind(cell)
            .bind(res as i32)
            .execute(pool)
            .await?;
        }
    }

    if row.inserted {
        ledger
            .append(
                vi_ledger::events::CASE_INGESTED,
                &json!({
                    "case_id": case_id,
                    "docket_number": c.docket_number,
                    "jurisdiction": jurisdiction,
                    "forum_method": forum_method,
                    "source_url": c.source_url,
                    "raw_hash": raw_hash,
                }),
            )
            .await?;
    }
    Ok((case_id, row.inserted))
}

/// Returns the opinion id and whether this poll is what created it.
pub async fn persist_opinion(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    case_id: Uuid,
    o: &NormalizedOpinion,
) -> Result<(Uuid, bool)> {
    // Identical text in the same case is one record, however many feeds
    // reported it: the unique key is (case_id, md5(full_text)).
    let row = sqlx::query_as::<_, (Uuid, bool)>(
        "INSERT INTO court_opinions
           (opinion_id, case_id, court_level, judge, citation, date_issued, full_text,
            text_completeness, source_url, source_ref)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (case_id, md5(full_text)) DO UPDATE SET
            citation = COALESCE(court_opinions.citation, EXCLUDED.citation),
            judge = COALESCE(court_opinions.judge, EXCLUDED.judge),
            date_issued = COALESCE(court_opinions.date_issued, EXCLUDED.date_issued),
            court_level = COALESCE(court_opinions.court_level, EXCLUDED.court_level),
            source_url = COALESCE(EXCLUDED.source_url, court_opinions.source_url),
            source_ref = COALESCE(court_opinions.source_ref, EXCLUDED.source_ref),
            text_completeness = EXCLUDED.text_completeness
         RETURNING opinion_id, (xmax = 0) AS inserted",
    )
    .bind(Uuid::new_v4())
    .bind(case_id)
    .bind(&o.court_level)
    .bind(&o.judge)
    .bind(&o.citation)
    .bind(o.date_issued)
    .bind(&o.text)
    .bind(o.completeness.as_str())
    .bind(&o.source_url)
    .bind(&o.source_ref)
    .fetch_one(pool)
    .await?;

    let (opinion_id, inserted) = row;

    // Complete text supersedes the snippet of the same source record. Leaving
    // both would let one opinion be counted twice, and would leave derived
    // leads pointing at a truncated document.
    if o.completeness == Completeness::Full {
        if let Some(source_ref) = o.source_ref.as_deref() {
            let superseded = sqlx::query(
                "DELETE FROM court_opinions
                  WHERE source_ref = $1 AND opinion_id <> $2
                    AND text_completeness <> 'full'",
            )
            .bind(source_ref)
            .bind(opinion_id)
            .execute(pool)
            .await?
            .rows_affected();
            if superseded > 0 {
                tracing::info!(
                    source_ref,
                    superseded,
                    "replaced partial opinion text with the complete document"
                );
            }
        }
    }

    if inserted {
        ledger
            .append(
                vi_ledger::events::OPINION_INGESTED,
                &json!({
                    "opinion_id": opinion_id,
                    "case_id": case_id,
                    "citation": o.citation,
                    "completeness": o.completeness.as_str(),
                    "source_ref": o.source_ref,
                    "text_hash": vi_ledger::hash_payload(&json!(o.text)),
                }),
            )
            .await?;
    }
    Ok((opinion_id, inserted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_sealed_and_blocked() {
        assert!(is_excluded("SEALED-1", &json!({})));
        assert!(is_excluded("X", &json!({"blocked": true})));
        assert!(is_excluded("jv-juvenile-matter", &json!({})));
        assert!(!is_excluded(
            "DEMO-2024-001",
            &json!({"docket_number": "DEMO-2024-001"})
        ));
    }

    #[test]
    fn fixture_poll_has_case_and_opinion() {
        let p = fixture_poll();
        assert_eq!(p.cases.len(), 1);
        assert_eq!(p.opinions.len(), 1);
        assert_eq!(p.cases[0].docket_number, "FIXTURE-2024-001");
        assert!(p.opinions[0].text.contains("body-worn camera"));
        assert_eq!(p.opinions[0].completeness, Completeness::Full);
    }

    #[test]
    fn completeness_is_explicit_about_partial_text() {
        assert!(!Completeness::Full.is_partial());
        assert!(Completeness::Snippet.is_partial());
        assert!(Completeness::Summary.is_partial());
        assert_eq!(Completeness::Snippet.as_str(), "snippet");
    }

    #[test]
    fn feed_names_are_stable() {
        assert_eq!(
            FeedSpec::Search {
                query: "Brady violation".into(),
                backfill: false
            }
            .name(),
            "courtlistener-search/brady-violation"
        );
        assert_eq!(
            FeedSpec::Atom {
                court: "ca9".into()
            }
            .name(),
            "courtlistener-feed/ca9"
        );
        assert_eq!(FeedSpec::Courts { pages: 1 }.name(), "courtlistener-courts");
    }

    #[test]
    fn unknown_feed_names_expand_to_nothing() {
        assert!(expand("nonsense").is_empty());
        assert!(expand("").is_empty());
    }

    #[test]
    fn a_query_can_be_named_directly() {
        let specs = expand("courtlistener-search/withheld-exculpatory-evidence");
        assert_eq!(
            specs,
            vec![FeedSpec::Search {
                query: "withheld exculpatory evidence".into(),
                backfill: false
            }]
        );
    }

    #[test]
    fn fixture_expands_to_the_fixture_feed() {
        assert_eq!(expand("fixture"), vec![FeedSpec::Fixture]);
    }

    /// `all` used to mean "whatever INGEST_SOURCES says", so the documented
    /// value `INGEST_SOURCES=all` asked the variable what it meant and
    /// overflowed the stack before reading a record.
    #[test]
    fn all_resolves_without_consulting_the_variable_that_may_name_it() {
        let specs = expand("all");
        assert!(!specs.is_empty(), "`all` must name real feeds");
        assert!(specs.iter().any(|s| matches!(s, FeedSpec::Courts { .. })));
        assert!(specs.iter().any(|s| matches!(s, FeedSpec::Search { .. })));
    }

    /// The configuration that crashed the service: `INGEST_SOURCES=all`.
    #[test]
    fn a_deployment_may_ask_for_all_feeds_by_name() {
        let specs = sources_from(Some("all"));
        assert!(!specs.is_empty());
        assert_eq!(specs, expand_defaults());
    }

    #[test]
    fn expansion_is_a_fixed_point() {
        // Expanding the names of expanded feeds yields the same feeds, so no
        // token can grow the list a second time around.
        let specs = expand("all");
        let names = specs
            .iter()
            .map(FeedSpec::name)
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(expand_list(&names), specs);
    }

    #[test]
    fn a_feed_named_twice_is_polled_once() {
        // `Vec::dedup` drops only neighbours, which left this list polling the
        // court registry twice every cycle.
        let specs = expand_list("courtlistener-courts,courtlistener-search,courtlistener-courts");
        let courts = specs
            .iter()
            .filter(|s| matches!(s, FeedSpec::Courts { .. }))
            .count();
        assert_eq!(courts, 1);
    }
}
