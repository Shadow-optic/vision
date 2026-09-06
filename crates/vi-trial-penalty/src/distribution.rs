use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

pub const EVENT_SNAPSHOT: &str = vi_ledger::events::TRIAL_PENALTY_SNAPSHOT;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PenaltyDistribution {
    pub n: i64,
    pub mean_ratio: Option<f64>,
    pub median_ratio: Option<f64>,
    pub p90_ratio: Option<f64>,
    pub p95_ratio: Option<f64>,
    pub mean_offer_months: Option<f64>,
    pub mean_sentence_months: Option<f64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HeatCell {
    pub cell: String,
    pub n: i64,
    pub mean_ratio: Option<f64>,
}

const BASE_SQL: &str = r#"
SELECT
  COUNT(*) AS n,
  AVG(ratio) AS mean_ratio,
  percentile_cont(0.5) WITHIN GROUP (ORDER BY ratio) AS median_ratio,
  percentile_cont(0.9) WITHIN GROUP (ORDER BY ratio) AS p90_ratio,
  percentile_cont(0.95) WITHIN GROUP (ORDER BY ratio) AS p95_ratio,
  AVG(plea_offer_months::float8) AS mean_offer_months,
  AVG(sentence_months::float8) AS mean_sentence_months
FROM (
  SELECT cc.*, cc.sentence_months::float / cc.plea_offer_months AS ratio
  FROM court_cases cc
  JOIN prosecutors p ON p.prosecutor_id = cc.prosecutor_id
  WHERE cc.plea_offered = true
    AND cc.plea_accepted = false
    AND cc.outcome = 'conviction'
    AND cc.plea_offer_months > 0
    AND cc.sentence_months > 0
    {filter}
) sub
"#;

fn sql_with_filter(filter: &str) -> String {
    BASE_SQL.replace("{filter}", filter)
}

pub async fn by_office(
    pool: &PgPool,
    ledger: &Ledger,
    office: &str,
    jurisdiction: Option<&str>,
) -> Result<(PenaltyDistribution, Uuid), crate::Error> {
    let sql = sql_with_filter("AND p.office = $1 AND ($2::text IS NULL OR p.jurisdiction = $2)");
    let dist: PenaltyDistribution = sqlx::query_as(&sql)
        .bind(office)
        .bind(jurisdiction)
        .fetch_one(pool)
        .await?;
    let id = snapshot(ledger, "office", office, jurisdiction, &dist).await?;
    Ok((dist, id))
}

pub async fn by_judge(
    pool: &PgPool,
    ledger: &Ledger,
    judge: &str,
    jurisdiction: Option<&str>,
) -> Result<(PenaltyDistribution, Uuid), crate::Error> {
    let sql = sql_with_filter("AND cc.judge = $1 AND ($2::text IS NULL OR p.jurisdiction = $2)");
    let dist: PenaltyDistribution = sqlx::query_as(&sql)
        .bind(judge)
        .bind(jurisdiction)
        .fetch_one(pool)
        .await?;
    let id = snapshot(ledger, "judge", judge, jurisdiction, &dist).await?;
    Ok((dist, id))
}

pub async fn heatmap(
    pool: &PgPool,
    resolution: u8,
    jurisdiction: Option<&str>,
) -> Result<Vec<HeatCell>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT c3.h3_cell AS cell,
                  COUNT(*) AS n,
                  AVG(cc.sentence_months::float / cc.plea_offer_months) AS mean_ratio
           FROM court_cases cc
           JOIN prosecutors p ON p.prosecutor_id = cc.prosecutor_id
           JOIN case_h3_cells c3 ON c3.case_id = cc.case_id
           WHERE c3.resolution = $1
             AND c3.cell_type = 'court'
             AND cc.plea_offered = true
             AND cc.plea_accepted = false
             AND cc.outcome = 'conviction'
             AND cc.plea_offer_months > 0
             AND cc.sentence_months > 0
             AND ($2::text IS NULL OR p.jurisdiction = $2)
           GROUP BY c3.h3_cell
           HAVING COUNT(*) >= 5
           ORDER BY mean_ratio DESC"#,
    )
    .bind(resolution as i32)
    .bind(jurisdiction)
    .fetch_all(pool)
    .await
}

async fn snapshot(
    ledger: &Ledger,
    scope: &str,
    key: &str,
    jurisdiction: Option<&str>,
    dist: &PenaltyDistribution,
) -> Result<Uuid, crate::Error> {
    let id = Uuid::new_v4();
    let payload = json!({
        "snapshot_id": id,
        "scope": scope,
        "key": key,
        "jurisdiction": jurisdiction,
        "distribution": dist,
    });
    ledger.append(EVENT_SNAPSHOT, &payload).await?;
    Ok(id)
}

/// What the pipeline's accumulation step did with one case.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Accumulation {
    /// The case carried disposition fields and was folded into its office's
    /// distribution (which was then recomputed and snapshotted).
    Accumulated { office: Option<String> },
    /// The case lacks the disposition fields the distributions are built
    /// from. Reported, counted, and never filled in with invented values.
    SkippedNoData { missing: Vec<&'static str> },
}

#[derive(sqlx::FromRow)]
struct DispositionRow {
    jurisdiction: String,
    office: Option<String>,
    plea_offered: Option<bool>,
    plea_accepted: Option<bool>,
    outcome: Option<String>,
    plea_offer_months: Option<i32>,
    sentence_months: Option<i32>,
}

/// Fold one case's disposition fields into the office-level distributions.
///
/// The distributions themselves are computed live over `court_cases`; the
/// observation row is the durable, auditable record of which cases carried
/// usable fields, and the office distribution is recomputed and snapshotted
/// so the ledger reflects what the new record changed. A case without those
/// fields is a counted skip, not a silent drop and never a fabricated row.
pub async fn accumulate_case(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
) -> Result<Accumulation, crate::Error> {
    let row = sqlx::query_as::<_, DispositionRow>(
        "SELECT c.jurisdiction, p.office, c.plea_offered, c.plea_accepted, c.outcome,
                c.plea_offer_months, c.sentence_months
           FROM court_cases c
           LEFT JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
          WHERE c.case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(Accumulation::SkippedNoData {
            missing: vec!["case"],
        });
    };

    let mut missing = Vec::new();
    if row.plea_offered.is_none() {
        missing.push("plea_offered");
    }
    if row.plea_accepted.is_none() {
        missing.push("plea_accepted");
    }
    if row.outcome.is_none() {
        missing.push("outcome");
    }
    if row.plea_offer_months.is_none() {
        missing.push("plea_offer_months");
    }
    if row.sentence_months.is_none() {
        missing.push("sentence_months");
    }
    if !missing.is_empty() {
        return Ok(Accumulation::SkippedNoData { missing });
    }

    sqlx::query(
        "INSERT INTO trial_penalty_observations
           (case_id, office, jurisdiction, plea_offered, plea_accepted, outcome,
            plea_offer_months, sentence_months)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (case_id) DO UPDATE SET
            office = EXCLUDED.office,
            jurisdiction = EXCLUDED.jurisdiction,
            plea_offered = EXCLUDED.plea_offered,
            plea_accepted = EXCLUDED.plea_accepted,
            outcome = EXCLUDED.outcome,
            plea_offer_months = EXCLUDED.plea_offer_months,
            sentence_months = EXCLUDED.sentence_months,
            observed_at = now()",
    )
    .bind(case_id)
    .bind(&row.office)
    .bind(&row.jurisdiction)
    .bind(row.plea_offered)
    .bind(row.plea_accepted)
    .bind(&row.outcome)
    .bind(row.plea_offer_months)
    .bind(row.sentence_months)
    .execute(pool)
    .await?;

    ledger
        .append(
            vi_ledger::events::CASE_DISPOSITION,
            &json!({
                "case_id": case_id,
                "office": row.office,
                "jurisdiction": row.jurisdiction,
                "plea_offered": row.plea_offered,
                "plea_accepted": row.plea_accepted,
                "outcome": row.outcome,
            }),
        )
        .await?;

    // Refresh the office distribution so the snapshot trail shows the effect
    // of the new observation. Offices are the public unit; judges stay
    // internal.
    if let Some(office) = row.office.as_deref() {
        by_office(pool, ledger, office, Some(&row.jurisdiction)).await?;
    }

    Ok(Accumulation::Accumulated { office: row.office })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_is_injected_once() {
        let sql = sql_with_filter("AND p.office = $1");
        assert!(sql.contains("AND p.office = $1"));
        assert!(!sql.contains("{filter}"));
    }
}
