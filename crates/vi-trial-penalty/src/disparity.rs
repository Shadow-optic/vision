use serde::Serialize;
use sqlx::PgPool;
use vi_correlation::{self, OddsResult as CoreOdds};

#[derive(Debug, Serialize)]
pub struct OddsResult {
    pub table: [u32; 4],
    pub odds_ratio: f64,
    pub ci95: (f64, f64),
    pub haldane_corrected: bool,
    pub formula: &'static str,
}

impl From<CoreOdds> for OddsResult {
    fn from(r: CoreOdds) -> Self {
        Self {
            table: r.table,
            odds_ratio: r.odds_ratio,
            ci95: r.ci95,
            haldane_corrected: r.haldane_corrected,
            formula: r.formula,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RaceComparison {
    pub charge_category: Option<String>,
    pub group_a: String,
    pub group_b: String,
    pub n_a: usize,
    pub n_b: usize,
    pub mean_ratio_a: f64,
    pub mean_ratio_b: f64,
    pub odds_ratio: Option<OddsResult>,
    pub interpretation: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct RaceRow {
    race: Option<String>,
    ratio: f64,
}

pub async fn racial_disparity(
    pool: &PgPool,
    charge_category: Option<&str>,
    group_a: &str,
    group_b: &str,
    severe_threshold: f64,
) -> Result<RaceComparison, sqlx::Error> {
    let groups = vec![group_a.to_string(), group_b.to_string()];

    let rows: Vec<RaceRow> = sqlx::query_as(
        r#"SELECT cc.defendant_race AS race,
                  cc.sentence_months::float / cc.plea_offer_months AS ratio
           FROM court_cases cc
           WHERE cc.plea_offered = true
             AND cc.plea_accepted = false
             AND cc.outcome = 'conviction'
             AND cc.plea_offer_months > 0
             AND cc.sentence_months > 0
             AND cc.defendant_race = ANY($1)
             AND ($2::text IS NULL OR cc.charge_category = $2)"#,
    )
    .bind(&groups)
    .bind(charge_category)
    .fetch_all(pool)
    .await?;

    let mut a_ratios = Vec::new();
    let mut b_ratios = Vec::new();
    let mut outcomes = Vec::new();
    let mut exposure = Vec::new();

    for r in rows {
        let Some(race) = r.race else { continue };
        let severe = r.ratio > severe_threshold;
        if race == group_a {
            a_ratios.push(r.ratio);
            outcomes.push(severe);
            exposure.push(true);
        } else if race == group_b {
            b_ratios.push(r.ratio);
            outcomes.push(severe);
            exposure.push(false);
        }
    }

    let n_a = a_ratios.len();
    let n_b = b_ratios.len();

    let odds = if n_a > 0 && n_b > 0 {
        vi_correlation::contingency(&outcomes, &exposure).map(|t| {
            let r = vi_correlation::odds_ratio(t[0], t[1], t[2], t[3]);
            OddsResult::from(r)
        })
    } else {
        None
    };

    let mean_a = average(&a_ratios);
    let mean_b = average(&b_ratios);

    let interpretation = format!(
        "Among {} defendants with trial-penalty data, {} (n={}) had mean penalty ratio {:.2}; {} (n={}) had {:.2}. \
         A penalty ratio > {:.1}x was coded as a 'severe trial penalty'. Correlation is not causation; race fields \
         are used only where lawfully sourced from public records.",
        charge_category.unwrap_or("all"),
        group_a,
        n_a,
        mean_a,
        group_b,
        n_b,
        mean_b,
        severe_threshold
    );

    Ok(RaceComparison {
        charge_category: charge_category.map(String::from),
        group_a: group_a.to_string(),
        group_b: group_b.to_string(),
        n_a,
        n_b,
        mean_ratio_a: mean_a,
        mean_ratio_b: mean_b,
        odds_ratio: odds,
        interpretation,
    })
}

fn average(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_empty_is_zero() {
        assert_eq!(average(&[]), 0.0);
        assert!((average(&[2.0, 4.0]) - 3.0).abs() < 1e-12);
    }
}
