use serde::Serialize;
use sqlx::PgPool;
use std::fmt::Write;

#[derive(Debug, Serialize, sqlx::FromRow)]
struct TypeRaw {
    finding_type: String,
    office_count: i64,
    state_count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TopEntry {
    pub name: Option<String>,
    pub count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct CaseCount {
    n: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeCompare {
    pub finding_type: String,
    pub office_count: i64,
    pub office_rate_per_1000: f64,
    pub state_rate_per_1000: f64,
    pub z_score: f64,
    pub p_value_one_tailed: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Fingerprint {
    pub office: String,
    pub jurisdiction: Option<String>,
    pub total_substantiated: i64,
    pub recent_5yr: i64,
    pub by_type: Vec<TypeCompare>,
    pub top_judges: Vec<TopEntry>,
    pub top_prosecutors: Vec<TopEntry>,
    pub interpretation: String,
}

pub async fn office_fingerprint(
    pool: &PgPool,
    office: &str,
    jurisdiction: Option<&str>,
) -> Result<Fingerprint, sqlx::Error> {
    let office_n = sqlx::query_as::<_, CaseCount>(
        "SELECT COUNT(*) AS n FROM court_cases cc
         JOIN prosecutors p USING (prosecutor_id)
         WHERE p.office = $1 AND ($2::text IS NULL OR p.jurisdiction = $2)",
    )
    .bind(office)
    .bind(jurisdiction)
    .fetch_one(pool)
    .await?
    .n;

    let state_n = sqlx::query_as::<_, CaseCount>(
        "SELECT COUNT(*) AS n FROM court_cases cc
         JOIN prosecutors p USING (prosecutor_id)
         WHERE p.office <> $1 AND ($2::text IS NULL OR p.jurisdiction = $2)",
    )
    .bind(office)
    .bind(jurisdiction)
    .fetch_one(pool)
    .await?
    .n;

    let rows: Vec<TypeRaw> = sqlx::query_as(
        "SELECT COALESCE(o.finding_type, s.finding_type) AS finding_type,
                COALESCE(o.cnt,0) AS office_count,
                COALESCE(s.cnt,0) AS state_count
         FROM (
             SELECT finding_type, COUNT(*) AS cnt
             FROM constitutional_findings
             WHERE office=$1 AND review_status='substantiated'
               AND ($2::text IS NULL OR jurisdiction=$2)
             GROUP BY finding_type
         ) o
         FULL OUTER JOIN (
             SELECT finding_type, COUNT(*) AS cnt
             FROM constitutional_findings
             WHERE office<>$1 AND review_status='substantiated'
               AND ($2::text IS NULL OR jurisdiction=$2)
             GROUP BY finding_type
         ) s USING (finding_type)",
    )
    .bind(office)
    .bind(jurisdiction)
    .fetch_all(pool)
    .await?;

    let recent_5yr: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM constitutional_findings
         WHERE office=$1 AND review_status='substantiated'
           AND ($2::text IS NULL OR jurisdiction=$2)
           AND finding_date >= CURRENT_DATE - INTERVAL '5 years'",
    )
    .bind(office)
    .bind(jurisdiction)
    .fetch_one(pool)
    .await?;

    let top_judges: Vec<TopEntry> = sqlx::query_as(
        "SELECT judge AS name, COUNT(*) AS count
         FROM constitutional_findings
         WHERE office=$1 AND review_status='substantiated'
           AND ($2::text IS NULL OR jurisdiction=$2)
         GROUP BY judge ORDER BY count DESC LIMIT 10",
    )
    .bind(office)
    .bind(jurisdiction)
    .fetch_all(pool)
    .await?;

    let top_prosecutors: Vec<TopEntry> = sqlx::query_as(
        "SELECT p.name, COUNT(*) AS count
         FROM constitutional_findings f
         LEFT JOIN prosecutors p USING (prosecutor_id)
         WHERE f.office=$1 AND f.review_status='substantiated'
           AND ($2::text IS NULL OR f.jurisdiction=$2)
         GROUP BY p.name ORDER BY count DESC LIMIT 10",
    )
    .bind(office)
    .bind(jurisdiction)
    .fetch_all(pool)
    .await?;

    let by_type: Vec<TypeCompare> = rows
        .into_iter()
        .map(|r| compare(&r, office_n, state_n))
        .collect();

    let interpretation = build_interpretation(&by_type, recent_5yr);

    Ok(Fingerprint {
        office: office.to_string(),
        jurisdiction: jurisdiction.map(String::from),
        total_substantiated: by_type.iter().map(|x| x.office_count).sum(),
        recent_5yr,
        by_type,
        top_judges,
        top_prosecutors,
        interpretation,
    })
}

/// Recompute an office's fingerprint and store it as the office's current
/// fingerprint row. The pipeline calls this after a case in the office is
/// ingested so the stored fingerprint is durable and auditable rather than a
/// warm-up nobody can inspect. The fingerprint reflects counsel-substantiated
/// findings only; an office with none gets an honest zero-count row.
pub async fn refresh_office_fingerprint(
    pool: &PgPool,
    office: &str,
    jurisdiction: Option<&str>,
) -> Result<Fingerprint, sqlx::Error> {
    let fp = office_fingerprint(pool, office, jurisdiction).await?;
    sqlx::query(
        "INSERT INTO monell_fingerprints (office, jurisdiction, fingerprint)
         VALUES ($1,$2,$3)
         ON CONFLICT (office) DO UPDATE SET
            jurisdiction = EXCLUDED.jurisdiction,
            fingerprint = EXCLUDED.fingerprint,
            computed_at = now()",
    )
    .bind(office)
    .bind(jurisdiction)
    .bind(serde_json::json!(&fp))
    .execute(pool)
    .await?;
    Ok(fp)
}

fn compare(r: &TypeRaw, n_office: i64, n_state: i64) -> TypeCompare {
    let o_rate = if n_office > 0 {
        r.office_count as f64 / n_office as f64 * 1000.0
    } else {
        0.0
    };
    let s_rate = if n_state > 0 {
        r.state_count as f64 / n_state as f64 * 1000.0
    } else {
        0.0
    };
    let (z, p) = two_prop_z(r.office_count, n_office, r.state_count, n_state);
    TypeCompare {
        finding_type: r.finding_type.clone(),
        office_count: r.office_count,
        office_rate_per_1000: o_rate,
        state_rate_per_1000: s_rate,
        z_score: z,
        p_value_one_tailed: p,
    }
}

pub(crate) fn two_prop_z(x1: i64, n1: i64, x2: i64, n2: i64) -> (f64, f64) {
    if n1 <= 0 || n2 <= 0 {
        return (0.0, 0.5);
    }
    let p1 = x1 as f64 / n1 as f64;
    let p2 = x2 as f64 / n2 as f64;
    let p = (x1 + x2) as f64 / (n1 + n2) as f64;
    let se2 = p * (1.0 - p) * (1.0 / n1 as f64 + 1.0 / n2 as f64);
    if se2 <= 0.0 {
        return (0.0, 0.5);
    }
    let se = se2.sqrt();
    let z = (p1 - p2) / se;
    let p_two_tailed = 1.0 - erf(z.abs() / 2.0_f64.sqrt());
    (z, p_two_tailed / 2.0)
}

/// Abramowitz & Stegun approximation of the error function.
pub(crate) fn erf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    sign * y
}

fn build_interpretation(by_type: &[TypeCompare], recent_5yr: i64) -> String {
    let mut s = String::new();
    let significant: Vec<&TypeCompare> = by_type
        .iter()
        .filter(|t| t.z_score > 2.576 && t.office_count > 0)
        .collect();

    if significant.is_empty() {
        write!(
            &mut s,
            "No substantiated violation type in this office currently shows a statistically significant excess over the state/jurisdiction baseline (z > 2.576)."
        )
        .ok();
    } else {
        write!(
            &mut s,
            "Statistically significant excess substantiated findings (z > 2.576): "
        )
        .ok();
        for t in &significant {
            let ty = &t.finding_type;
            let count = t.office_count;
            let z = t.z_score;
            write!(&mut s, "{ty} ({count} findings, z={z:.2}); ").ok();
        }
    }
    write!(
        &mut s,
        " {recent_5yr} substantiated findings in the last 5 years."
    )
    .ok();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erf_zero() {
        assert!(erf(0.0).abs() < 1e-7);
    }

    #[test]
    fn erf_known() {
        // erf(1) ≈ 0.8427
        assert!((erf(1.0) - 0.8427).abs() < 1e-3);
    }

    #[test]
    fn equal_proportions_z_near_zero() {
        let (z, p) = two_prop_z(10, 100, 10, 100);
        assert!(z.abs() < 1e-9);
        assert!((p - 0.5).abs() < 1e-6);
    }

    #[test]
    fn empty_n_is_neutral() {
        let (z, p) = two_prop_z(1, 0, 0, 10);
        assert_eq!(z, 0.0);
        assert_eq!(p, 0.5);
    }

    #[test]
    fn interpretation_without_signal() {
        let text = build_interpretation(&[], 0);
        assert!(text.contains("No substantiated"));
    }
}
