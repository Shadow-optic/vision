//! Abuse Score from substantiated public-record findings only.
//! Pending TrustScript flags never enter the score (defamation guardrail).
#![forbid(unsafe_code)]

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

use crate::Error;

pub const FORMULA: &str = "min(100, 12*min(findings,4) + 8*min(flags,3) + 6*min(sources,3) + (10 if recent_5yr>0 else 0))";

#[derive(Debug, Clone, Serialize)]
pub struct AbuseScore {
    pub actor_id: Uuid,
    pub score: f64,
    pub substantiated_findings: i64,
    pub substantiated_flags: i64,
    pub corroboration_sources: i64,
    pub recent_5yr: i64,
    pub formula: &'static str,
    pub components: Value,
    pub snapshot_id: Option<Uuid>,
}

pub fn compute_points(findings: i64, flags: i64, sources: i64, recent_5yr: i64) -> f64 {
    let finding_pts = 12.0 * findings.min(4) as f64;
    let flag_pts = 8.0 * flags.min(3) as f64;
    let source_pts = 6.0 * sources.min(3) as f64;
    let recency_pts = if recent_5yr > 0 { 10.0 } else { 0.0 };
    (finding_pts + flag_pts + source_pts + recency_pts).min(100.0)
}

pub async fn score_actor(
    pool: &PgPool,
    ledger: Option<&Ledger>,
    actor_id: Uuid,
) -> Result<AbuseScore, Error> {
    let actor = crate::entity::get(pool, actor_id).await?;

    let findings: i64 = if let Some(pid) = actor.prosecutor_id {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM constitutional_findings
             WHERE prosecutor_id = $1 AND review_status = 'substantiated'",
        )
        .bind(pid)
        .fetch_one(pool)
        .await?
    } else {
        0
    };

    let flags: i64 = if let Some(pid) = actor.prosecutor_id {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM abuse_flags
             WHERE prosecutor_id = $1 AND review_status = 'substantiated'",
        )
        .bind(pid)
        .fetch_one(pool)
        .await?
    } else {
        0
    };

    let recent_5yr: i64 = if let Some(pid) = actor.prosecutor_id {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM constitutional_findings
             WHERE prosecutor_id = $1 AND review_status = 'substantiated'
               AND finding_date >= CURRENT_DATE - INTERVAL '5 years'",
        )
        .bind(pid)
        .fetch_one(pool)
        .await?
    } else {
        0
    };

    let finding_types: Vec<String> = if let Some(pid) = actor.prosecutor_id {
        sqlx::query_scalar(
            "SELECT DISTINCT finding_type FROM constitutional_findings
             WHERE prosecutor_id = $1 AND review_status = 'substantiated'",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };
    let flag_labels: Vec<String> = if let Some(pid) = actor.prosecutor_id {
        sqlx::query_scalar(
            "SELECT DISTINCT label FROM abuse_flags
             WHERE prosecutor_id = $1 AND review_status = 'substantiated'",
        )
        .bind(pid)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };

    let mut sources = finding_types;
    for label in flag_labels {
        if !sources.iter().any(|s| s == &label) {
            sources.push(label);
        }
    }
    let corroboration = sources.len() as i64;
    let score = compute_points(findings, flags, corroboration, recent_5yr);
    let components = json!({
        "finding_points": 12.0 * findings.min(4) as f64,
        "flag_points": 8.0 * flags.min(3) as f64,
        "source_points": 6.0 * corroboration.min(3) as f64,
        "recency_points": if recent_5yr > 0 { 10.0 } else { 0.0 },
        "sources": sources,
        "note": "Pending flags and unreviewed findings are excluded.",
    });

    let mut snapshot_id = None;
    if let Some(ledger) = ledger {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO actor_score_snapshots
             (snapshot_id, actor_id, score, substantiated_findings, substantiated_flags,
              corroboration_sources, recent_5yr, formula, components)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(id)
        .bind(actor_id)
        .bind(score)
        .bind(findings as i32)
        .bind(flags as i32)
        .bind(corroboration as i32)
        .bind(recent_5yr as i32)
        .bind(FORMULA)
        .bind(&components)
        .execute(pool)
        .await?;
        ledger
            .append(
                vi_ledger::events::ABUSE_SCORE,
                &json!({
                    "actor_id": actor_id,
                    "snapshot_id": id,
                    "score": score,
                    "substantiated_findings": findings,
                }),
            )
            .await?;
        snapshot_id = Some(id);
    }

    Ok(AbuseScore {
        actor_id,
        score,
        substantiated_findings: findings,
        substantiated_flags: flags,
        corroboration_sources: corroboration,
        recent_5yr,
        formula: FORMULA,
        components,
        snapshot_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_one_finding_one_source_recent() {
        let s = compute_points(1, 0, 1, 1);
        assert!((s - 28.0).abs() < 1e-9);
    }

    #[test]
    fn pending_only_is_zero() {
        assert_eq!(compute_points(0, 0, 0, 0), 0.0);
    }

    #[test]
    fn caps_at_one_hundred() {
        assert_eq!(compute_points(99, 99, 99, 99), 100.0);
    }
}
