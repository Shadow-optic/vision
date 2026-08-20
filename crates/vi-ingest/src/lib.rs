//! Ingestion: every source implements `Source`. Records are normalized,
//! H3-tagged, hash-provenanced into the Root Ledger, then upserted.
//! PACER (fee-bearing, credentials), state portals, and exoneration-registry
//! imports plug in as additional `Source` impls — nothing downstream changes.
#![forbid(unsafe_code)]

use anyhow::Result;
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::Value;
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

pub trait Source {
    fn name(&self) -> &str;
    #[allow(async_fn_in_trait)]
    async fn poll(&self) -> Result<Vec<NormalizedCase>>;
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
            http: reqwest::Client::new(),
            base: "https://www.courtlistener.com".into(),
            token,
        }
    }

    async fn get(&self, url: &str) -> Result<ClPage> {
        let mut req = self.http.get(url);
        if let Some(t) = &self.token {
            req = req.header("Authorization", format!("Token {t}"));
        }
        Ok(req.send().await?.error_for_status()?.json().await?)
    }
}

impl Source for CourtListenerClient {
    fn name(&self) -> &str {
        "courtlistener"
    }

    /// TODO(phase-1): select endpoint(s) per data type (dockets/opinions), map
    /// CL fields → NormalizedCase, checkpoint the `next` cursor in Postgres,
    /// and respect CL rate limits + API terms. Skeleton fetches one page.
    async fn poll(&self) -> Result<Vec<NormalizedCase>> {
        let page = self
            .get(&format!(
                "{}/api/rest/v3/dockets/?order_by=-date_modified",
                self.base
            ))
            .await?;
        let mut out = Vec::new();
        for r in page.results {
            let Some(docket) = r.get("docket_number").and_then(Value::as_str) else {
                continue;
            };
            out.push(NormalizedCase {
                docket_number: docket.to_string(),
                jurisdiction: r
                    .get("court_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                court_level: None,
                charge_category: None,
                filed: r
                    .get("date_filed")
                    .and_then(Value::as_str)
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
                court_lat: None,
                court_lng: None,
                source_url: r.get("resource_uri").and_then(Value::as_str).map(|u| {
                    if u.starts_with("http") {
                        u.to_string()
                    } else {
                        format!("{}{u}", self.base)
                    }
                }),
                raw: r,
            });
        }
        let _ = page.next;
        Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
