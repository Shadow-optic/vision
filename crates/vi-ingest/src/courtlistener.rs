//! CourtListener sources (Free Law Project, public domain records).
//!
//! Four feeds, deliberately different in what they need and what they cost:
//!
//! * [`CourtRegistry`] — the courts list. No credentials. Refreshed rarely,
//!   it is what lets an ingested record be placed in a forum at all.
//! * [`SearchFeed`] — the search API. No credentials. Structured records with
//!   a docket number, court, date, judge, and a text snippet. This is the feed
//!   that runs by default, so the platform ingests real records out of the box.
//! * [`CourtFeed`] — a court's Atom feed. No credentials. Newest opinions for
//!   one court, with no query bias.
//! * [`ApiClient`] — the dockets and opinions endpoints. These require a token
//!   and are the only way to obtain complete opinion text, so a deployment with
//!   `CL_API_TOKEN` set gets `text_completeness = 'full'` records.
//!
//! Snippet-bearing feeds mark their opinions as partial. Downstream engines
//! that read opinion text derive *leads*, and a lead missing from a snippet is
//! a lead nobody looked for yet — not a finding of absence.
#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    is_excluded, Completeness, CourtRecord, NormalizedCase, NormalizedOpinion, PollResult, Source,
};

pub const BASE: &str = "https://www.courtlistener.com";
const USER_AGENT: &str =
    "VisionInjustice/0.1 (public-records accountability research; +https://github.com/Shadow-optic/vision)";

/// Deliberately conservative: these feeds are a public good and anonymous
/// access is rate limited. One page per feed per cycle, with a pause between
/// requests, keeps a long-running deployment inside a courteous budget.
const PAGE_SIZE: usize = 20;
const POLITE_DELAY_MS: u64 = 250;

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("building the CourtListener HTTP client")
}

async fn polite_pause() {
    tokio::time::sleep(std::time::Duration::from_millis(POLITE_DELAY_MS)).await;
}

/// GET with bounded retries. 429 and transport errors back off; 4xx other than
/// 429 fail immediately, because retrying an unauthorized or malformed request
/// only burns someone else's quota.
async fn get_text(client: &reqwest::Client, url: &str, token: Option<&str>) -> Result<String> {
    let mut last = String::new();
    for attempt in 0..3u32 {
        let mut req = client.get(url);
        if let Some(t) = token {
            req = req.header("Authorization", format!("Token {t}"));
        }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    let wait = 1000 * u64::from(attempt + 1);
                    tracing::warn!(url, wait_ms = wait, "rate limited; backing off");
                    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                    last = format!("{status} (rate limited)");
                    continue;
                }
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    bail!(
                        "{url} returned {status}: this endpoint needs CL_API_TOKEN. \
                         The search and Atom feeds work without one."
                    );
                }
                if !status.is_success() {
                    bail!("{url} returned {status}");
                }
                return Ok(resp.text().await?);
            }
            Err(e) => {
                last = e.to_string();
                tokio::time::sleep(std::time::Duration::from_millis(
                    300 * u64::from(attempt + 1),
                ))
                .await;
            }
        }
    }
    bail!("GET {url} failed after 3 attempts: {last}")
}

async fn get_json(client: &reqwest::Client, url: &str, token: Option<&str>) -> Result<Value> {
    let body = get_text(client, url, token).await?;
    serde_json::from_str(&body).with_context(|| format!("{url} did not return JSON"))
}

fn absolute(url: &str) -> String {
    if url.starts_with("http") {
        url.to_string()
    } else {
        format!("{BASE}{url}")
    }
}

fn date(value: Option<&str>) -> Option<NaiveDate> {
    let s = value?;
    NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok()
}

/// `/opinion/11435101/some-slug/` -> `11435101`.
fn opinion_id_from_url(url: &str) -> Option<String> {
    let rest = url.split("/opinion/").nth(1)?;
    let id = rest.split('/').next()?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(id.to_string())
}

/// A docket number as printed in an opinion's text: "No. 24-7676".
///
/// Lowercased once up front: this runs over complete opinions, and scanning a
/// fresh lowercase copy per character turns a long opinion into a stall.
fn docket_from_text(text: &str) -> Option<String> {
    // `to_ascii_lowercase` is byte-length preserving, so offsets still line up
    // with `text` even when the opinion contains non-ASCII characters.
    let lower = text.to_ascii_lowercase();
    for marker in ["case no.", "no.", "nos."] {
        let mut from = 0;
        while let Some(rel) = lower[from..].find(marker) {
            let after = from + rel + marker.len();
            let candidate: String = text[after..]
                .chars()
                .skip_while(|c| c.is_whitespace())
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '.' | '/'))
                .collect();
            let trimmed = candidate.trim_end_matches('.').to_string();
            if trimmed.len() >= 4 && trimmed.chars().any(|c| c.is_ascii_digit()) {
                return Some(trimmed);
            }
            from = after;
        }
    }
    None
}

// ===== Courts registry =====

/// The courts list. Cheap, stable, and a prerequisite for placing any record
/// in a forum, so it runs first in a default cycle.
///
/// Courts still in use are what live feeds can produce, and there are a few
/// hundred of them against several thousand historical entries, so the crawl
/// is scoped to those unless `CL_COURTS_ALL` asks for everything.
pub struct CourtRegistry {
    client: reqwest::Client,
    name: String,
    /// Pages per poll; the endpoint serves 20 courts per page.
    pages: usize,
    in_use_only: bool,
}

/// One court as the courts endpoint describes it.
fn court_record(r: Value) -> Option<CourtRecord> {
    let court_id = r.get("id").and_then(Value::as_str)?.to_string();
    let full_name = r
        .get("full_name")
        .and_then(Value::as_str)
        .unwrap_or(&court_id)
        .to_string();
    Some(CourtRecord {
        full_name,
        short_name: r
            .get("short_name")
            .and_then(Value::as_str)
            .map(str::to_string),
        citation_string: r
            .get("citation_string")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        source_class: r
            .get("jurisdiction")
            .and_then(Value::as_str)
            .map(str::to_string),
        in_use: r.get("in_use").and_then(Value::as_bool).unwrap_or(false),
        parent_court: r
            .get("parent_court")
            .and_then(Value::as_str)
            .map(str::to_string),
        start_date: date(r.get("start_date").and_then(Value::as_str)),
        end_date: date(r.get("end_date").and_then(Value::as_str)),
        source_url: Some(format!("{BASE}/api/rest/v4/courts/{court_id}/")),
        raw: r,
        court_id,
    })
}

/// Ask the source about one court.
///
/// The registry crawl is scoped to courts the source marks `in_use`, and that
/// flag turns out to omit courts still handing down the opinions we ingest —
/// `txctapp6` among them. Rather than guess a forum from the shape of an id,
/// which is how "ariz" becomes Arkansas, ask for the court by name. One
/// request per unplaced court, then it is in the registry for good.
pub async fn fetch_court(court_id: &str) -> Result<Option<CourtRecord>> {
    if court_id.is_empty()
        || !court_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Ok(None);
    }
    let client = http()?;
    let url = format!("{BASE}/api/rest/v4/courts/{court_id}/");
    let token = std::env::var("CL_API_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let body = get_json(&client, &url, token.as_deref()).await?;
    Ok(court_record(body))
}

impl CourtRegistry {
    pub fn new(pages: usize) -> Result<Self> {
        Ok(Self {
            client: http()?,
            name: "courtlistener-courts".into(),
            pages: pages.max(1),
            in_use_only: !matches!(
                std::env::var("CL_COURTS_ALL").unwrap_or_default().trim(),
                "1" | "true" | "yes" | "on"
            ),
        })
    }

    fn head_url(&self) -> String {
        if self.in_use_only {
            format!("{BASE}/api/rest/v4/courts/?in_use=true")
        } else {
            format!("{BASE}/api/rest/v4/courts/")
        }
    }
}

impl Source for CourtRegistry {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &'static str {
        "registry"
    }

    fn label(&self) -> String {
        "CourtListener courts registry".into()
    }

    async fn poll(&self, cursor: Option<&str>) -> Result<PollResult> {
        let mut url = cursor
            .filter(|c| c.contains("/courts/"))
            .map(str::to_string)
            .unwrap_or_else(|| self.head_url());

        let mut courts = Vec::new();
        let mut next = None;
        for page in 0..self.pages {
            if page > 0 {
                polite_pause().await;
            }
            let body = get_json(&self.client, &url, None).await?;
            let results = body
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            courts.extend(results.into_iter().filter_map(court_record));
            next = body.get("next").and_then(Value::as_str).map(str::to_string);
            match &next {
                Some(n) => url = n.clone(),
                None => break,
            }
        }

        Ok(PollResult {
            courts,
            next_cursor: next,
            ..Default::default()
        })
    }
}

// ===== Search feed (no credentials) =====

/// One saved query against the search API.
///
/// Queries are configuration, not doctrine: they decide which slice of the
/// public record gets looked at first, and nothing about what is concluded.
pub struct SearchFeed {
    client: reqwest::Client,
    name: String,
    query: String,
    /// When set, follow the cursor backwards through history. Off by default:
    /// a live feed should keep re-reading the head, where new records appear.
    backfill: bool,
}

impl SearchFeed {
    pub fn new(query: &str, backfill: bool) -> Result<Self> {
        let query = query.trim();
        if query.is_empty() {
            bail!("a search feed needs a query");
        }
        Ok(Self {
            client: http()?,
            name: format!("courtlistener-search/{}", slug(query)),
            query: query.to_string(),
            backfill,
        })
    }

    fn head_url(&self) -> String {
        let q = urlencode(&self.query);
        format!(
            "{BASE}/api/rest/v4/search/?q={q}&type=o&order_by=dateFiled%20desc&page_size={PAGE_SIZE}"
        )
    }
}

pub fn slug(input: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in input.trim().to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches('-').chars().take(48).collect()
}

fn urlencode(input: &str) -> String {
    let mut out = String::new();
    for b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

impl Source for SearchFeed {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &'static str {
        "search"
    }

    fn label(&self) -> String {
        format!("CourtListener search: \u{201c}{}\u{201d}", self.query)
    }

    async fn poll(&self, cursor: Option<&str>) -> Result<PollResult> {
        let url = match (self.backfill, cursor) {
            (true, Some(c)) if c.contains("/search/") => c.to_string(),
            _ => self.head_url(),
        };
        let body = get_json(&self.client, &url, None).await?;
        let results = body
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut cases = Vec::new();
        let mut opinions = Vec::new();
        let mut skipped = 0u64;

        for r in results {
            let court_id = r.get("court_id").and_then(Value::as_str).unwrap_or("");
            let docket_id = r.get("docket_id").and_then(Value::as_i64);
            let docket = r
                .get("docketNumber")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| docket_id.map(|id| format!("cl-docket:{id}")));
            let Some(docket) = docket else {
                skipped += 1;
                continue;
            };
            if is_excluded(&docket, &r) {
                skipped += 1;
                continue;
            }

            let case_name = r.get("caseName").and_then(Value::as_str).unwrap_or("");
            let judge = r
                .get("judge")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let filed = date(r.get("dateFiled").and_then(Value::as_str));
            let case_url = r.get("absolute_url").and_then(Value::as_str).map(absolute);

            cases.push(NormalizedCase {
                docket_number: docket.clone(),
                jurisdiction: court_id.to_string(),
                source_court_id: Some(court_id.to_string()).filter(|s| !s.is_empty()),
                court_level: None,
                charge_category: None,
                judge: judge.clone(),
                filed,
                court_lat: None,
                court_lng: None,
                source_url: case_url.clone(),
                raw: r.clone(),
            });

            for op in r
                .get("opinions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                let snippet = op
                    .get("snippet")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if snippet.is_empty() {
                    skipped += 1;
                    continue;
                }
                let source_ref = op
                    .get("id")
                    .and_then(Value::as_i64)
                    .map(|id| format!("cl-opinion:{id}"));
                opinions.push(NormalizedOpinion {
                    docket_number: Some(docket.clone()),
                    citation: Some(if case_name.is_empty() {
                        docket.clone()
                    } else {
                        case_name.to_string()
                    }),
                    court_level: None,
                    judge: judge.clone(),
                    date_issued: filed,
                    text: snippet,
                    completeness: Completeness::Snippet,
                    source_url: case_url.clone(),
                    source_ref,
                });
            }
        }

        Ok(PollResult {
            cases,
            opinions,
            skipped,
            next_cursor: body.get("next").and_then(Value::as_str).map(str::to_string),
            ..Default::default()
        })
    }
}

// ===== Court Atom feed (no credentials) =====

/// Newest opinions for one court, straight from its Atom feed.
///
/// The feed carries no docket number, so one is recovered from the opinion
/// text when it is printed there and otherwise synthesised from the source
/// record id. A synthetic docket is labelled as such rather than guessed at.
pub struct CourtFeed {
    client: reqwest::Client,
    name: String,
    court: String,
}

impl CourtFeed {
    pub fn new(court: &str) -> Result<Self> {
        let court = court.trim().to_ascii_lowercase();
        if court.is_empty() || !court.chars().all(|c| c.is_ascii_alphanumeric()) {
            bail!("court id must be alphanumeric, got '{court}'");
        }
        Ok(Self {
            client: http()?,
            name: format!("courtlistener-feed/{court}"),
            court,
        })
    }
}

impl Source for CourtFeed {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &'static str {
        "atom"
    }

    fn label(&self) -> String {
        format!("CourtListener Atom feed: {}", self.court)
    }

    async fn poll(&self, _cursor: Option<&str>) -> Result<PollResult> {
        let url = format!("{BASE}/feed/court/{}/", self.court);
        let xml = get_text(&self.client, &url, None).await?;
        let entries = crate::atom::parse(&xml)?;

        let mut cases = Vec::new();
        let mut opinions = Vec::new();
        let mut skipped = 0u64;

        for entry in entries {
            let text = entry.summary_text();
            if text.trim().is_empty() {
                skipped += 1;
                continue;
            }
            let source_ref = opinion_id_from_url(&entry.link).map(|id| format!("cl-opinion:{id}"));
            let docket = docket_from_text(&text)
                .or_else(|| source_ref.clone())
                .unwrap_or_else(|| format!("cl-feed:{}", crate::hash_key(&entry.link)));
            let raw = json!({
                "feed": url,
                "court_id": self.court,
                "title": entry.title,
                "link": entry.link,
                "published": entry.published,
                "author": entry.author,
                "category": entry.category,
            });
            if is_excluded(&docket, &raw) {
                skipped += 1;
                continue;
            }

            let published = date(entry.published.as_deref());
            cases.push(NormalizedCase {
                docket_number: docket.clone(),
                jurisdiction: self.court.clone(),
                source_court_id: Some(self.court.clone()),
                court_level: None,
                charge_category: None,
                judge: None,
                filed: published,
                court_lat: None,
                court_lng: None,
                source_url: Some(entry.link.clone()),
                raw,
            });
            opinions.push(NormalizedOpinion {
                docket_number: Some(docket),
                citation: Some(entry.title.clone()),
                court_level: None,
                judge: None,
                date_issued: published,
                text,
                completeness: Completeness::Summary,
                source_url: Some(entry.link),
                source_ref,
            });
        }

        Ok(PollResult {
            cases,
            opinions,
            skipped,
            next_cursor: None,
            ..Default::default()
        })
    }
}

// ===== Authenticated API (complete opinion text) =====

#[derive(Debug, Deserialize)]
struct ApiPage {
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    results: Vec<Value>,
}

/// Dockets and opinions endpoints. Requires `CL_API_TOKEN`; in exchange the
/// opinion text is complete, so derived leads are not limited to a snippet.
pub struct ApiClient {
    client: reqwest::Client,
    name: String,
    token: String,
}

impl ApiClient {
    pub fn new(token: String) -> Result<Self> {
        if token.trim().is_empty() {
            bail!("courtlistener API source requires CL_API_TOKEN");
        }
        Ok(Self {
            client: http()?,
            name: "courtlistener".into(),
            token,
        })
    }

    async fn page(&self, url: &str) -> Result<ApiPage> {
        let body = get_json(&self.client, url, Some(&self.token)).await?;
        Ok(serde_json::from_value(body)?)
    }
}

fn map_docket(r: Value) -> Option<NormalizedCase> {
    let docket = r
        .get("docket_number")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    if is_excluded(&docket, &r) {
        return None;
    }
    let court_id = r
        .get("court_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            r.get("court")
                .and_then(Value::as_str)
                .and_then(|u| u.trim_end_matches('/').rsplit('/').next())
                .map(str::to_string)
        });
    Some(NormalizedCase {
        docket_number: docket,
        jurisdiction: court_id.clone().unwrap_or_else(|| "unknown".into()),
        source_court_id: court_id,
        court_level: None,
        charge_category: None,
        judge: r
            .get("assigned_to_str")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        filed: date(r.get("date_filed").and_then(Value::as_str)),
        court_lat: None,
        court_lng: None,
        source_url: r.get("absolute_url").and_then(Value::as_str).map(absolute),
        raw: r,
    })
}

fn map_opinion(r: &Value) -> Option<NormalizedOpinion> {
    if r.get("blocked").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let text = r
        .get("plain_text")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| r.get("html_with_citations").and_then(Value::as_str))
        .or_else(|| r.get("html").and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return None;
    }
    let docket = r
        .pointer("/cluster/docket_number")
        .and_then(Value::as_str)
        .or_else(|| r.get("docket_number").and_then(Value::as_str))
        .map(str::to_string);
    Some(NormalizedOpinion {
        docket_number: docket,
        citation: r
            .get("citation")
            .and_then(Value::as_str)
            .or_else(|| r.pointer("/cluster/case_name").and_then(Value::as_str))
            .map(str::to_string),
        court_level: None,
        judge: r
            .get("author_str")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        date_issued: date(
            r.get("date_created")
                .and_then(Value::as_str)
                .or_else(|| r.pointer("/cluster/date_filed").and_then(Value::as_str)),
        ),
        text,
        completeness: Completeness::Full,
        source_url: r.get("absolute_url").and_then(Value::as_str).map(absolute),
        source_ref: r
            .get("id")
            .and_then(Value::as_i64)
            .map(|id| format!("cl-opinion:{id}")),
    })
}

impl Source for ApiClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &'static str {
        "api"
    }

    fn label(&self) -> String {
        "CourtListener dockets and opinions (authenticated)".into()
    }

    async fn poll(&self, cursor: Option<&str>) -> Result<PollResult> {
        let docket_url = cursor
            .filter(|c| c.contains("/dockets/"))
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!("{BASE}/api/rest/v4/dockets/?order_by=-date_modified&page_size={PAGE_SIZE}")
            });
        let dockets = self.page(&docket_url).await?;
        let cases: Vec<_> = dockets.results.into_iter().filter_map(map_docket).collect();

        polite_pause().await;

        let opinion_url =
            format!("{BASE}/api/rest/v4/opinions/?order_by=-date_modified&page_size={PAGE_SIZE}");
        let opinions = match self.page(&opinion_url).await {
            Ok(p) => p.results.iter().filter_map(map_opinion).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "opinions endpoint failed; keeping docket results");
                Vec::new()
            }
        };

        Ok(PollResult {
            cases,
            opinions,
            next_cursor: dockets.next,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_stable_and_bounded() {
        assert_eq!(slug("Brady violation"), "brady-violation");
        assert_eq!(slug("  fabricated   evidence!!  "), "fabricated-evidence");
        assert!(slug(&"x".repeat(200)).len() <= 48);
    }

    #[test]
    fn urlencoding_keeps_queries_intact() {
        assert_eq!(urlencode("brady violation"), "brady+violation");
        assert_eq!(urlencode("\"exact phrase\""), "%22exact+phrase%22");
    }

    #[test]
    fn opinion_ids_come_from_urls() {
        assert_eq!(
            opinion_id_from_url("https://www.courtlistener.com/opinion/10967451/x-v-y/").as_deref(),
            Some("10967451")
        );
        assert_eq!(opinion_id_from_url("/docket/123/x/"), None);
    }

    #[test]
    fn docket_numbers_are_recovered_from_opinion_text() {
        assert_eq!(
            docket_from_text("UNITED STATES COURT OF APPEALS No. 24-7676 CONCERNED PARENTS")
                .as_deref(),
            Some("24-7676")
        );
        assert_eq!(
            docket_from_text("IN THE COURT OF CHANCERY C.A. No. 2026-1021-BWD ").as_deref(),
            Some("2026-1021-BWD")
        );
        assert_eq!(docket_from_text("no docket printed here"), None);
    }

    #[test]
    fn search_feed_rejects_an_empty_query() {
        assert!(SearchFeed::new("   ", false).is_err());
    }

    #[test]
    fn court_feed_rejects_a_path_traversal() {
        assert!(CourtFeed::new("../../etc/passwd").is_err());
        assert!(CourtFeed::new("ca9").is_ok());
    }

    #[test]
    fn api_client_requires_a_token() {
        assert!(ApiClient::new(String::new()).is_err());
    }

    #[test]
    fn blocked_records_never_map() {
        let r = json!({"docket_number": "1:24-cv-1", "blocked": true});
        assert!(map_docket(r).is_none());
        assert!(map_opinion(&json!({"plain_text": "text", "blocked": true})).is_none());
    }

    #[test]
    fn authenticated_opinions_are_complete() {
        let mapped = map_opinion(&json!({
            "id": 42,
            "plain_text": "An opinion.",
            "docket_number": "X-1",
            "author_str": "Smith"
        }))
        .expect("maps");
        assert_eq!(mapped.completeness, Completeness::Full);
        assert_eq!(mapped.source_ref.as_deref(), Some("cl-opinion:42"));
    }
}
