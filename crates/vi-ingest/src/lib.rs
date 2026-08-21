//! Ingestion: every source implements `Source`. Records are normalized,
//! H3-tagged, hash-provenanced into the Root Ledger, then upserted.
//! PACER (fee-bearing, credentials), state portals, and exoneration-registry
//! imports plug in as additional `Source` impls — nothing downstream changes.
#![forbid(unsafe_code)]

use anyhow::{bail, Result};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NormalizedCase {
    pub docket_number: String,
    pub jurisdiction: String,
    pub court_level: Option<String>,
    pub charge_category: Option<String>,
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
    pub full_text: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PollResult {
    pub cases: Vec<NormalizedCase>,
    pub opinions: Vec<NormalizedOpinion>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub source: String,
    pub cases_persisted: u64,
    pub opinions_persisted: u64,
    pub skipped: u64,
    pub next_cursor: Option<String>,
}

pub trait Source {
    fn name(&self) -> &str;
    #[allow(async_fn_in_trait)]
    async fn poll(&self, cursor: Option<&str>) -> Result<PollResult>;
}

/// Sealed, juvenile, expunged, or CourtListener-blocked records are excluded.
pub fn is_excluded(docket: &str, raw: &Value) -> bool {
    if raw.get("blocked").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    let blob = format!("{docket} {raw}").to_ascii_lowercase();
    blob.contains("sealed") || blob.contains("juvenile") || blob.contains("expunged")
}

async fn load_cursor(pool: &PgPool, source: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT next_url FROM ingest_cursors WHERE source = $1",
    )
    .bind(source)
    .fetch_optional(pool)
    .await?
    .flatten())
}

async fn save_cursor(
    pool: &PgPool,
    source: &str,
    next: Option<&str>,
    count: i32,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO ingest_cursors (source, next_url, last_polled_at, last_count, last_error)
         VALUES ($1,$2,now(),$3,$4)
         ON CONFLICT (source) DO UPDATE SET
            next_url = EXCLUDED.next_url,
            last_polled_at = EXCLUDED.last_polled_at,
            last_count = EXCLUDED.last_count,
            last_error = EXCLUDED.last_error",
    )
    .bind(source)
    .bind(next)
    .bind(count)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_cursors(pool: &PgPool) -> Result<Vec<Value>> {
    let rows = sqlx::query_scalar::<_, Value>(
        r#"SELECT jsonb_build_object(
             'source', source, 'next_url', next_url,
             'last_polled_at', last_polled_at, 'last_count', last_count,
             'last_error', last_error)
           FROM ingest_cursors ORDER BY source"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn execute<S: Source>(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    source: &S,
) -> Result<RunReport> {
    let name = source.name();
    let cursor = load_cursor(pool, name).await?;
    match source.poll(cursor.as_deref()).await {
        Ok(poll) => {
            let mut skipped = 0u64;
            let mut cases_n = 0u64;
            let mut opinions_n = 0u64;
            for c in &poll.cases {
                if is_excluded(&c.docket_number, &c.raw) {
                    skipped += 1;
                    continue;
                }
                persist_case(pool, ledger, c).await?;
                cases_n += 1;
            }
            for o in &poll.opinions {
                if o.full_text.trim().is_empty() {
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
                        persist_case(
                            pool,
                            ledger,
                            &NormalizedCase {
                                docket_number: docket.to_string(),
                                jurisdiction: "unknown".into(),
                                court_level: o.court_level.clone(),
                                charge_category: None,
                                filed: o.date_issued,
                                court_lat: None,
                                court_lng: None,
                                source_url: o.source_url.clone(),
                                raw: json!({"from": "opinion", "docket_number": docket}),
                            },
                        )
                        .await?
                    }
                };
                persist_opinion(pool, ledger, case_id, o).await?;
                opinions_n += 1;
            }
            save_cursor(
                pool,
                name,
                poll.next_cursor.as_deref(),
                (cases_n + opinions_n) as i32,
                None,
            )
            .await?;
            Ok(RunReport {
                source: name.to_string(),
                cases_persisted: cases_n,
                opinions_persisted: opinions_n,
                skipped,
                next_cursor: poll.next_cursor,
            })
        }
        Err(e) => {
            save_cursor(pool, name, cursor.as_deref(), 0, Some(&e.to_string())).await?;
            Err(e)
        }
    }
}

pub async fn run_named(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    source: &str,
) -> Result<RunReport> {
    match source {
        "fixture" => {
            let src = FixtureSource;
            execute(pool, ledger, &src).await
        }
        "courtlistener" => {
            let src = CourtListenerClient::new(std::env::var("CL_API_TOKEN").ok());
            execute(pool, ledger, &src).await
        }
        other => bail!("unknown ingest source '{other}' (expected fixture|courtlistener)"),
    }
}

async fn lookup_case_id(pool: &PgPool, docket: &str) -> Result<Option<Uuid>> {
    Ok(
        sqlx::query_scalar::<_, Uuid>("SELECT case_id FROM court_cases WHERE docket_number = $1")
            .bind(docket)
            .fetch_optional(pool)
            .await?,
    )
}

// -------- CourtListener (REST v3) — needs CL_API_TOKEN for production volume.

pub struct CourtListenerClient {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClPage {
    next: Option<String>,
    results: Vec<Value>,
}

impl CourtListenerClient {
    pub fn new(token: Option<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent("VisionInjustice/0.1 (accountability research; +https://github.com/Shadow-optic/vision)")
                .build()
                .expect("reqwest client"),
            base: "https://www.courtlistener.com".into(),
            token,
        }
    }

    async fn get(&self, url: &str) -> Result<ClPage> {
        let mut last_err = None;
        for attempt in 0..3 {
            let mut req = self.http.get(url);
            if let Some(t) = &self.token {
                req = req.header("Authorization", format!("Token {t}"));
            }
            match req.send().await {
                Ok(resp) => {
                    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        tokio::time::sleep(std::time::Duration::from_millis(400 * (attempt + 1)))
                            .await;
                        continue;
                    }
                    let resp = resp.error_for_status()?;
                    return Ok(resp.json().await?);
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt + 1))).await;
                }
            }
        }
        bail!("courtlistener GET failed: {last_err:?}")
    }
}

fn map_docket(base: &str, r: Value) -> Option<NormalizedCase> {
    let docket = r.get("docket_number").and_then(Value::as_str)?.to_string();
    if is_excluded(&docket, &r) {
        return None;
    }
    Some(NormalizedCase {
        docket_number: docket,
        jurisdiction: r
            .get("court_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        court_level: r.get("court").and_then(Value::as_str).map(str::to_string),
        charge_category: None,
        filed: r
            .get("date_filed")
            .and_then(Value::as_str)
            .and_then(|s| NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok()),
        court_lat: None,
        court_lng: None,
        source_url: r
            .get("absolute_url")
            .and_then(Value::as_str)
            .or_else(|| r.get("resource_uri").and_then(Value::as_str))
            .map(|u| {
                if u.starts_with("http") {
                    u.to_string()
                } else {
                    format!("{base}{u}")
                }
            }),
        raw: r,
    })
}

fn map_opinion(base: &str, r: &Value) -> Option<NormalizedOpinion> {
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
        .to_string();
    if text.trim().is_empty() {
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
            .map(str::to_string),
        date_issued: r
            .get("date_created")
            .and_then(Value::as_str)
            .and_then(|s| NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok()),
        full_text: text,
        source_url: r.get("absolute_url").and_then(Value::as_str).map(|u| {
            if u.starts_with("http") {
                u.to_string()
            } else {
                format!("{base}{u}")
            }
        }),
    })
}

impl Source for CourtListenerClient {
    fn name(&self) -> &str {
        "courtlistener"
    }

    async fn poll(&self, cursor: Option<&str>) -> Result<PollResult> {
        let docket_url = cursor
            .filter(|c| c.contains("/dockets/"))
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "{}/api/rest/v3/dockets/?order_by=-date_modified&page_size=20",
                    self.base
                )
            });
        let docket_page = self.get(&docket_url).await?;
        let cases: Vec<_> = docket_page
            .results
            .into_iter()
            .filter_map(|r| map_docket(&self.base, r))
            .collect();

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let opinion_url = format!(
            "{}/api/rest/v3/opinions/?order_by=-date_modified&page_size=20",
            self.base
        );
        let opinion_page = self.get(&opinion_url).await.unwrap_or(ClPage {
            next: None,
            results: vec![],
        });
        let opinions: Vec<_> = opinion_page
            .results
            .iter()
            .filter_map(|r| map_opinion(&self.base, r))
            .collect();

        Ok(PollResult {
            cases,
            opinions,
            next_cursor: docket_page.next,
        })
    }
}

// -------- Fixture source (CI / local, no network) --------

pub struct FixtureSource;

impl Source for FixtureSource {
    fn name(&self) -> &str {
        "fixture"
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
        court_level: Some("superior".into()),
        charge_category: Some("drug".into()),
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
        full_text: "The court referenced body-worn camera footage and a laboratory report. \
                    A 911 call recording was discussed. Chain of custody was not produced."
            .into(),
        source_url: Some("https://example.test/fixture/opinion".into()),
    };
    PollResult {
        cases: vec![case],
        opinions: vec![opinion],
        next_cursor: None,
    }
}

#[derive(Debug, sqlx::FromRow)]
struct UpsertRow {
    case_id: Uuid,
    inserted: bool,
}

/// Upsert + H3 ladder + ledger provenance for one record.
pub async fn persist_case(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    c: &NormalizedCase,
) -> Result<Uuid> {
    let (court_h3, incident_h3) = match (c.court_lat, c.court_lng) {
        (Some(lat), Some(lng)) => (vi_geo::cell_for(lat, lng, 8).ok(), None::<String>),
        _ => (None, None),
    };
    let raw_hash = vi_ledger::hash_payload(&c.raw);
    let filed = c.filed.map(|d| {
        Utc.from_utc_datetime(&d.and_time(NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")))
    });

    let row = sqlx::query_as::<_, UpsertRow>(
        "INSERT INTO court_cases
           (case_id, docket_number, jurisdiction, court_level, charge_category,
            filing_date, court_location_lat, court_location_lng,
            court_h3_cell, incident_h3_cell, source_url, raw_data, hash)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
         ON CONFLICT (docket_number) DO UPDATE SET
            source_url = COALESCE(EXCLUDED.source_url, court_cases.source_url),
            raw_data   = EXCLUDED.raw_data,
            hash       = EXCLUDED.hash,
            court_h3_cell = COALESCE(EXCLUDED.court_h3_cell, court_cases.court_h3_cell)
         RETURNING case_id, (xmax = 0) AS inserted",
    )
    .bind(Uuid::new_v4())
    .bind(&c.docket_number)
    .bind(&c.jurisdiction)
    .bind(&c.court_level)
    .bind(&c.charge_category)
    .bind(filed)
    .bind(c.court_lat)
    .bind(c.court_lng)
    .bind(&court_h3)
    .bind(&incident_h3)
    .bind(&c.source_url)
    .bind(&c.raw)
    .bind(&raw_hash)
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
                &serde_json::json!({
                    "case_id": case_id,
                    "docket_number": c.docket_number,
                    "source_url": c.source_url,
                    "raw_hash": raw_hash,
                }),
            )
            .await?;
    }
    Ok(case_id)
}

pub async fn persist_opinion(
    pool: &PgPool,
    ledger: &vi_ledger::Ledger,
    case_id: Uuid,
    o: &NormalizedOpinion,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO court_opinions
           (opinion_id, case_id, court_level, judge, citation, date_issued, full_text)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT DO NOTHING
         RETURNING opinion_id",
    )
    .bind(id)
    .bind(case_id)
    .bind(&o.court_level)
    .bind(&o.judge)
    .bind(&o.citation)
    .bind(o.date_issued)
    .bind(&o.full_text)
    .fetch_optional(pool)
    .await?;

    let opinion_id = inserted.unwrap_or(id);
    if inserted.is_some() {
        ledger
            .append(
                vi_ledger::events::OPINION_INGESTED,
                &json!({
                    "opinion_id": opinion_id,
                    "case_id": case_id,
                    "citation": o.citation,
                    "text_hash": vi_ledger::hash_payload(&json!(o.full_text)),
                }),
            )
            .await?;
    }
    Ok(opinion_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_case_holds_raw() {
        let c = NormalizedCase {
            docket_number: "X-1".into(),
            jurisdiction: "CA".into(),
            court_level: None,
            charge_category: None,
            filed: None,
            court_lat: None,
            court_lng: None,
            source_url: None,
            raw: json!({"docket_number": "X-1"}),
        };
        assert_eq!(c.docket_number, "X-1");
    }

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
        assert!(p.opinions[0].full_text.contains("body-worn camera"));
    }

    #[test]
    fn map_docket_skips_blocked() {
        let r = json!({
            "docket_number": "1:24-cv-1",
            "court_id": "cand",
            "blocked": true
        });
        assert!(map_docket("https://www.courtlistener.com", r).is_none());
    }

    #[test]
    fn map_opinion_requires_text() {
        let empty = json!({"plain_text": "  ", "docket_number": "X"});
        assert!(map_opinion("https://www.courtlistener.com", &empty).is_none());
        let ok = json!({
            "plain_text": "An opinion.",
            "docket_number": "X",
            "author_str": "Smith"
        });
        let mapped = map_opinion("https://www.courtlistener.com", &ok).unwrap();
        assert_eq!(mapped.docket_number.as_deref(), Some("X"));
        assert_eq!(mapped.judge.as_deref(), Some("Smith"));
    }
}
