//! Per-case signal extraction. Each signal is a raw numeric value per case,
//! drawn from a different engine's sub-threshold output. Signals that are
//! absent (no data) are simply missing — never fabricated.
//!
//! Signals:
//!   abuse        — Σ severity weight over PENDING abuse flags (vi-trustscript)
//!   constitution — total hit count over PENDING constitution screens
//!   brady        — gap ratio of the latest non-rejected Brady recon run
//!                  (gaps_found / expected items); absent when nothing expected
//!   geo          — k-ring (k=1) conviction-rate z-score of the case's
//!                  incident cell vs the corpus conviction rate (vi-geo);
//!                  absent without an H3 cell or a ring under MIN_RING_CASES
#![forbid(unsafe_code)]

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

/// Stouffer weights. Abuse flags are direct allegations (heaviest); the geo
/// signal is ecological — it says something about a place, not a case — so it
/// carries the least weight.
pub const W_ABUSE: f64 = 1.5;
pub const W_CONSTITUTION: f64 = 1.0;
pub const W_BRADY: f64 = 1.0;
pub const W_GEO: f64 = 0.5;

/// Minimum resolved-outcome cases in a k-ring before a z-score is meaningful.
pub const MIN_RING_CASES: usize = 5;
/// H3 ring size for the geo disparity signal.
pub const GEO_K: u32 = 1;

pub const S_ABUSE: &str = "abuse";
pub const S_CONSTITUTION: &str = "constitution";
pub const S_BRADY: &str = "brady";
pub const S_GEO: &str = "geo";

#[derive(Debug, Clone)]
pub struct RawSignal {
    pub case_id: Uuid,
    pub name: &'static str,
    pub value: f64,
    pub weight: f64,
}

/// TrustScript severities → numeric weights.
pub fn severity_weight(severity: &str) -> f64 {
    match severity {
        "low" => 1.0,
        "medium" => 2.0,
        "high" => 3.0,
        "critical" => 4.0,
        _ => 1.0,
    }
}

pub async fn extract_signals(pool: &PgPool) -> Result<Vec<RawSignal>, Error> {
    let mut out = Vec::new();
    extract_abuse(pool, &mut out).await?;
    extract_constitution(pool, &mut out).await?;
    extract_brady(pool, &mut out).await?;
    extract_geo(pool, &mut out).await?;
    Ok(out)
}

async fn extract_abuse(pool: &PgPool, out: &mut Vec<RawSignal>) -> Result<(), Error> {
    // Pending flags only: substantiated flags are already published evidence,
    // rejected flags are void.
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT case_id, severity FROM abuse_flags
          WHERE review_status = 'pending' AND case_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    let mut totals: HashMap<Uuid, f64> = HashMap::new();
    for (case_id, severity) in rows {
        *totals.entry(case_id).or_default() += severity_weight(&severity);
    }
    for (case_id, value) in totals {
        out.push(RawSignal {
            case_id,
            name: S_ABUSE,
            value,
            weight: W_ABUSE,
        });
    }
    Ok(())
}

async fn extract_constitution(pool: &PgPool, out: &mut Vec<RawSignal>) -> Result<(), Error> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT case_id, COALESCE(SUM(hit_count), 0)::bigint
           FROM constitution_screens
          WHERE review_status = 'pending' AND case_id IS NOT NULL
          GROUP BY case_id",
    )
    .fetch_all(pool)
    .await?;
    for (case_id, hits) in rows {
        out.push(RawSignal {
            case_id,
            name: S_CONSTITUTION,
            value: hits as f64,
            weight: W_CONSTITUTION,
        });
    }
    Ok(())
}

async fn extract_brady(pool: &PgPool, out: &mut Vec<RawSignal>) -> Result<(), Error> {
    // Latest non-rejected recon run per case; ratio needs a nonzero expected
    // count, otherwise the case contributes no brady signal.
    let rows: Vec<(Uuid, i32, i64)> = sqlx::query_as(
        "SELECT DISTINCT ON (r.case_id)
                r.case_id,
                r.gaps_found,
                (SELECT COUNT(*) FROM expected_evidence_items e
                  WHERE e.case_id = r.case_id) AS expected_count
           FROM brady_recon_runs r
          WHERE r.review_status != 'rejected'
          ORDER BY r.case_id, r.run_at DESC",
    )
    .fetch_all(pool)
    .await?;
    for (case_id, gaps_found, expected_count) in rows {
        if expected_count <= 0 {
            continue;
        }
        out.push(RawSignal {
            case_id,
            name: S_BRADY,
            value: gaps_found as f64 / expected_count as f64,
            weight: W_BRADY,
        });
    }
    Ok(())
}

async fn extract_geo(pool: &PgPool, out: &mut Vec<RawSignal>) -> Result<(), Error> {
    let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT case_id, incident_h3_cell, outcome
           FROM court_cases
          WHERE incident_h3_cell IS NOT NULL
            AND outcome IN ('conviction','acquittal','dismissal')",
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let convictions = rows
        .iter()
        .filter(|r| r.2.as_deref() == Some("conviction"))
        .count() as f64;
    let p_all = convictions / rows.len() as f64;
    if p_all <= 0.0 || p_all >= 1.0 {
        return Ok(()); // no variance → z-score undefined for every case
    }

    let mut by_cell: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (_, cell, _)) in rows.iter().enumerate() {
        by_cell.entry(cell.as_str()).or_default().push(i);
    }

    for (case_id, cell, _) in &rows {
        let Ok(ring) = vi_geo::k_ring(cell, GEO_K) else {
            continue; // unparseable stored cell — skip, never fabricate
        };
        let (mut n, mut c) = (0usize, 0usize);
        for rc in &ring {
            if let Some(idxs) = by_cell.get(rc.as_str()) {
                for &i in idxs {
                    n += 1;
                    if rows[i].2.as_deref() == Some("conviction") {
                        c += 1;
                    }
                }
            }
        }
        if n < MIN_RING_CASES {
            continue;
        }
        let rate = c as f64 / n as f64;
        let se = (p_all * (1.0 - p_all) / n as f64).sqrt();
        if se <= 0.0 {
            continue;
        }
        out.push(RawSignal {
            case_id: *case_id,
            name: S_GEO,
            value: (rate - p_all) / se,
            weight: W_GEO,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_weights_are_ordered() {
        assert!(severity_weight("low") < severity_weight("medium"));
        assert!(severity_weight("medium") < severity_weight("high"));
        assert!(severity_weight("high") < severity_weight("critical"));
        assert_eq!(severity_weight("unknown"), severity_weight("low"));
    }
}
