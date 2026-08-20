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
