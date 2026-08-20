use crate::error::ApiError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::{events, Ledger};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub ledger: Ledger,
}

pub async fn health() -> &'static str {
    "ok"
}

pub async fn ready(State(st): State<AppState>) -> Result<&'static str, ApiError> {
    vi_db::ping(&st.pool).await?;
    Ok("ready")
}

pub async fn backfill_h3(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let rows: Vec<(Uuid, Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT case_id, court_location_lat, court_location_lng FROM court_cases
         WHERE court_location_lat IS NOT NULL AND court_location_lng IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    let mut n = 0u64;
    for (id, lat, lng) in rows {
        let (Some(lat), Some(lng)) = (lat, lng) else {
            continue;
        };
        if let Ok(cell) = vi_geo::cell_for(lat, lng, 8) {
            sqlx::query(
                "UPDATE court_cases SET court_h3_cell = COALESCE(court_h3_cell, $1) WHERE case_id = $2",
            )
            .bind(&cell)
            .bind(id)
            .execute(pool)
            .await?;
        }
        for (res, cell) in vi_geo::ladder(lat, lng) {
            sqlx::query(
                "INSERT INTO case_h3_cells (case_id, h3_cell, resolution, cell_type)
                 VALUES ($1,$2,$3,'court') ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(cell)
            .bind(res as i32)
            .execute(pool)
            .await?;
        }
        n += 1;
    }
    Ok(n)
}

// ---------- case search ----------

#[derive(Deserialize)]
pub struct SearchQ {
    pub q: String,
    pub limit: Option<i64>,
}

pub async fn search(
    State(st): State<AppState>,
    Query(q): Query<SearchQ>,
) -> Result<Json<Value>, ApiError> {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let rows = sqlx::query_scalar::<_, Value>(
        r#"SELECT jsonb_build_object(
             'case_id', c.case_id, 'docket_number', c.docket_number,
             'jurisdiction', c.jurisdiction, 'outcome', c.outcome,
             'citation', o.citation, 'date_issued', o.date_issued,
             'rank', ts_rank(o.tsv, query))
           FROM court_opinions o
           JOIN court_cases c USING (case_id),
                websearch_to_tsquery('english', $1) query
           WHERE o.tsv @@ query
           ORDER BY ts_rank(o.tsv, query) DESC
           LIMIT $2"#,
    )
    .bind(&q.q)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({ "results": rows })))
}

pub async fn case_context(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let ctx = vi_db::case_context(&st.pool, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(ctx))
}

// ---------- prosecutor stats ----------

pub async fn prosecutor_stats(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let row = sqlx::query_scalar::<_, Value>(
        r#"SELECT jsonb_build_object(
             'prosecutor_id', p.prosecutor_id, 'name', p.name, 'office', p.office,
             'total_cases',        COUNT(c.case_id),
             'convictions',        COUNT(c.case_id) FILTER (WHERE c.outcome = 'conviction'),
             'acquittals',         COUNT(c.case_id) FILTER (WHERE c.outcome = 'acquittal'),
             'dismissals',         COUNT(c.case_id) FILTER (WHERE c.outcome = 'dismissal'),
             'avg_plea_months',    AVG(c.sentence_months) FILTER (WHERE c.plea_accepted),
             'avg_trial_months',   AVG(c.sentence_months)
                                       FILTER (WHERE NOT c.plea_accepted AND c.outcome = 'conviction'),
             'flags_pending',      (SELECT COUNT(*) FROM abuse_flags f
                                     WHERE f.prosecutor_id = p.prosecutor_id
                                       AND f.review_status = 'pending'),
             'flags_substantiated',(SELECT COUNT(*) FROM abuse_flags f
                                     WHERE f.prosecutor_id = p.prosecutor_id
                                       AND f.review_status = 'substantiated'))
           FROM prosecutors p
           LEFT JOIN court_cases c ON c.prosecutor_id = p.prosecutor_id
           WHERE p.prosecutor_id = $1
           GROUP BY p.prosecutor_id, p.name, p.office"#,
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(row))
}

// ---------- H3 cell stats ----------

pub async fn cell_stats(
    State(st): State<AppState>,
    Path(cell): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let row = sqlx::query_scalar::<_, Value>(
        r#"SELECT jsonb_build_object(
             'cell', $1,
             'total', COUNT(*),
             'convictions', COUNT(*) FILTER (WHERE outcome = 'conviction'),
             'dismissals',  COUNT(*) FILTER (WHERE outcome = 'dismissal'),
             'avg_sentence_months', AVG(sentence_months) FILTER (WHERE outcome = 'conviction'))
           FROM court_cases
           WHERE court_h3_cell = $1 OR incident_h3_cell = $1"#,
    )
    .bind(&cell)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(row))
}

// ---------- TrustScript rules ----------

#[derive(Deserialize)]
pub struct NewRule {
    pub name: String,
    pub source: String,
}

pub async fn list_rules(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rules = sqlx::query_as::<_, (Uuid, String, String, bool)>(
        "SELECT rule_id, name, source, enabled FROM abuse_rules ORDER BY created_at",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({
        "rules": rules.iter().map(|(id, name, src, en)| json!({
            "rule_id": id, "name": name, "source": src, "enabled": en
        })).collect::<Vec<_>>()
    })))
}

pub async fn create_rule(
    State(st): State<AppState>,
    Json(body): Json<NewRule>,
) -> Result<Json<Value>, ApiError> {
    let parsed = vi_trustscript::parse_rule(&body.source)
        .map_err(|e| ApiError::bad_req(format!("invalid TrustScript: {e}")))?;
    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO abuse_rules (name, source) VALUES ($1, $2) RETURNING rule_id",
    )
    .bind(&body.name)
    .bind(&body.source)
    .fetch_one(&st.pool)
    .await?;
    st.ledger
        .append(
            events::RULE_CREATED,
            &json!({"rule_id": id, "name": body.name, "flag": parsed.flag}),
        )
        .await?;
    Ok(Json(json!({ "rule_id": id })))
}

#[derive(Deserialize)]
pub struct RunRules {
    pub case_id: Uuid,
}

pub async fn run_rules(
    State(st): State<AppState>,
    Json(body): Json<RunRules>,
) -> Result<Json<Value>, ApiError> {
    let ctx = vi_db::case_context(&st.pool, body.case_id)
        .await?
        .ok_or_else(ApiError::not_found)?;

    let rows = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT rule_id, source FROM abuse_rules WHERE enabled",
    )
    .fetch_all(&st.pool)
    .await?;

    let prosecutor_id = ctx
        .pointer("/case/prosecutor_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok());
    let office = ctx
        .pointer("/case/office")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut fired = Vec::new();
    for (rule_id, src) in rows {
        let Ok(rule) = vi_trustscript::parse_rule(&src) else {
            continue;
        };
        let Some(flag) = vi_trustscript::evaluate(&rule, &ctx) else {
            continue;
        };

        let flag_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO abuse_flags
               (case_id, rule_id, prosecutor_id, office, label, severity, explanation)
             VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING flag_id",
        )
        .bind(body.case_id)
        .bind(rule_id)
        .bind(prosecutor_id)
        .bind(&office)
        .bind(&flag.label)
        .bind(format!("{:?}", flag.severity).to_lowercase())
        .bind(json!(flag.matched))
        .fetch_one(&st.pool)
        .await?;

        st.ledger
            .append(
                events::ABUSE_FLAG,
                &json!({
                    "flag_id": flag_id,
                    "case_id": body.case_id,
                    "rule_id": rule_id,
                    "label": flag.label,
                    "explanation_hash": vi_ledger::hash_payload(&json!(flag.matched)),
                }),
            )
            .await?;
        fired.push(json!({
            "flag_id": flag_id,
            "label": flag.label,
            "severity": flag.severity,
            "matched": flag.matched,
            "review_status": "pending"
        }));
    }
    Ok(Json(json!({ "case_id": body.case_id, "flags": fired })))
}

#[derive(Deserialize)]
pub struct FlagsQ {
    pub office: Option<String>,
    pub status: Option<String>,
}

pub async fn list_flags(
    State(st): State<AppState>,
    Query(q): Query<FlagsQ>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_scalar::<_, Value>(
        r#"SELECT jsonb_build_object(
             'flag_id', flag_id, 'case_id', case_id, 'office', office,
             'label', label, 'severity', severity,
             'review_status', review_status, 'created_at', created_at)
           FROM abuse_flags
           WHERE ($1::text IS NULL OR office = $1)
             AND ($2::text IS NULL OR review_status = $2)
           ORDER BY created_at DESC LIMIT 200"#,
    )
    .bind(q.office)
    .bind(q.status)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({ "flags": rows })))
}

// ---------- zero-day simulation ----------

#[derive(Deserialize)]
pub struct SimReq {
    pub priors: vi_sim::CasePriors,
    pub strategy: vi_sim::Strategy,
    pub trials: u32,
    pub seed: Option<u64>,
}

pub fn unit_interval(v: f64, name: &str) -> Result<(), ApiError> {
    (v.is_finite() && (0.0..=1.0).contains(&v))
        .then_some(())
        .ok_or_else(|| ApiError::bad_req(format!("{name} must be in [0,1]")))
}

pub async fn simulate(
    State(st): State<AppState>,
    Json(req): Json<SimReq>,
) -> Result<Json<Value>, ApiError> {
    let p = &req.priors;
    let s = &req.strategy;
    unit_interval(p.evidence_strength, "evidence_strength")?;
    unit_interval(p.charge_severity, "charge_severity")?;
    unit_interval(p.prior_record, "prior_record")?;
    unit_interval(p.judge_propensity, "judge_propensity")?;
    unit_interval(p.prosecutor_aggressiveness, "prosecutor_aggressiveness")?;
    unit_interval(p.jury_propensity, "jury_propensity")?;
    unit_interval(s.plea_discount, "plea_discount")?;
    unit_interval(s.suppression_bonus, "suppression_bonus")?;
    unit_interval(s.acquittal_bonus, "acquittal_bonus")?;
    if p.base_plea_months < 0.0 || p.base_trial_months < 0.0 {
        return Err(ApiError::bad_req("base months must be >= 0"));
    }
    let trials = req.trials.clamp(100, 1_000_000);
    let seed = req.seed.unwrap_or(42);

    let (p2, s2) = (p.clone(), s.clone());
    let dist = tokio::task::spawn_blocking(move || vi_sim::simulate(&p2, &s2, trials, seed))
        .await
        .map_err(ApiError::internal)?;

    let sim_id = Uuid::new_v4();
    let params = json!({"priors": p, "strategy": s, "trials": trials, "seed": seed});
    sqlx::query(
        "INSERT INTO simulations (sim_id, seed, trials, params, result) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(sim_id)
    .bind(seed.to_string())
    .bind(trials as i32)
    .bind(&params)
    .bind(json!(&dist))
    .execute(&st.pool)
    .await?;

    let entry = st
        .ledger
        .append(
            events::SIMULATION_RESULT,
            &json!({
                "sim_id": sim_id,
                "input_hash": vi_ledger::hash_payload(&params),
                "seed": seed,
                "trials": trials,
            }),
        )
        .await?;

    Ok(Json(json!({
        "simulation_id": sim_id,
        "ledger_seq": entry.seq,
        "distribution": dist
    })))
}

// ---------- ledger audit ----------

pub async fn verify_ledger(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(st.ledger.verify().await?)))
}

// ---------- correlation ----------

#[derive(Deserialize)]
pub struct PearsonBody {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

pub async fn pearson(Json(body): Json<PearsonBody>) -> Result<Json<Value>, ApiError> {
    let r = vi_correlation::pearson(&body.x, &body.y).ok_or_else(|| {
        ApiError::bad_req("need paired samples with n >= 4 and non-zero variance")
    })?;
    Ok(Json(json!(r)))
}

#[derive(Deserialize)]
pub struct OddsBody {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
}

pub async fn odds(Json(body): Json<OddsBody>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(vi_correlation::odds_ratio(
        body.a, body.b, body.c, body.d
    ))))
}

#[derive(Deserialize)]
pub struct OfficeQ {
    pub office: String,
    pub jurisdiction: Option<String>,
}

pub async fn plea_sentence_corr(
    State(st): State<AppState>,
    Query(q): Query<OfficeQ>,
) -> Result<Json<Value>, ApiError> {
    let rows: Vec<(Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT cc.plea_offer_months, cc.sentence_months
         FROM court_cases cc
         JOIN prosecutors p ON p.prosecutor_id = cc.prosecutor_id
         WHERE p.office = $1
           AND ($2::text IS NULL OR p.jurisdiction = $2)
           AND cc.plea_offer_months IS NOT NULL
           AND cc.sentence_months IS NOT NULL",
    )
    .bind(&q.office)
    .bind(q.jurisdiction.as_deref())
    .fetch_all(&st.pool)
    .await?;
    let mut x = Vec::new();
    let mut y = Vec::new();
    for (a, b) in rows {
        if let (Some(a), Some(b)) = (a, b) {
            x.push(a as f64);
            y.push(b as f64);
        }
    }
    let r = vi_correlation::pearson(&x, &y)
        .ok_or_else(|| ApiError::bad_req("insufficient paired observations (n >= 4 required)"))?;
    Ok(Json(json!({ "office": q.office, "correlation": r })))
}

// ---------- LASM ----------

pub async fn lasm_package(
    State(st): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT COALESCE(docket_number, case_id::text),
                (SELECT o.citation FROM court_opinions o WHERE o.case_id = c.case_id LIMIT 1)
         FROM court_cases c WHERE c.case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(&st.pool)
    .await?;
    let (docket, citation) = row.ok_or_else(ApiError::not_found)?;
    let caption = citation.unwrap_or_else(|| docket.clone());

    let flags: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT label, severity, explanation FROM abuse_flags
         WHERE case_id = $1 AND review_status = 'substantiated'
         ORDER BY created_at",
    )
    .bind(case_id)
    .fetch_all(&st.pool)
    .await?;

    let summaries: Vec<vi_lasm::FlagSummary> = flags
        .into_iter()
        .map(|(label, severity, expl)| {
            let matched = expl
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            vi_lasm::FlagSummary {
                label,
                severity,
                matched,
            }
        })
        .collect();

    let ledger_rows = st.ledger.entries_for_case(case_id).await?;
    let ledger: Vec<vi_lasm::LedgerRef> = ledger_rows
        .into_iter()
        .map(|e| vi_lasm::LedgerRef {
            seq: e.seq,
            event_type: e.event_type,
            entry_hash: e.entry_hash,
        })
        .collect();

    let md = vi_lasm::render(&vi_lasm::EvidencePackage {
        caption: &caption,
        docket_number: &docket,
        flags: summaries,
        ledger,
    })
    .map_err(ApiError::internal)?;
    Ok((StatusCode::OK, [("content-type", "text/markdown")], md).into_response())
}

// ---------- Monell Atlas ----------

pub async fn atlas_create_finding(
    State(st): State<AppState>,
    Json(body): Json<vi_monell_atlas::FindingInput>,
) -> Result<Json<Value>, ApiError> {
    let id = vi_monell_atlas::record_finding(&st.pool, &st.ledger, &body).await?;
    Ok(Json(json!({ "finding_id": id })))
}

pub async fn atlas_fingerprint(
    State(st): State<AppState>,
    Query(q): Query<OfficeQ>,
) -> Result<Json<Value>, ApiError> {
    let fp =
        vi_monell_atlas::stats::office_fingerprint(&st.pool, &q.office, q.jurisdiction.as_deref())
            .await?;
    Ok(Json(json!(fp)))
}

pub async fn atlas_report(
    State(st): State<AppState>,
    Query(q): Query<OfficeQ>,
) -> Result<Response, ApiError> {
    let fp =
        vi_monell_atlas::stats::office_fingerprint(&st.pool, &q.office, q.jurisdiction.as_deref())
            .await?;
    let md = vi_monell_atlas::report::render(&fp).map_err(ApiError::internal)?;
    Ok((StatusCode::OK, [("content-type", "text/markdown")], md).into_response())
}

#[derive(Deserialize)]
pub struct ReviewBody {
    pub status: String,
    pub reviewer_id: Option<Uuid>,
}

pub async fn atlas_review_finding(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, ApiError> {
    vi_monell_atlas::review_finding(&st.pool, &st.ledger, id, &body.status, body.reviewer_id)
        .await?;
    Ok(Json(json!({ "finding_id": id, "status": body.status })))
}

// ---------- Brady Reconciliation ----------

pub async fn brady_derive(
    State(st): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let items = vi_brady_recon::extractor::derive_expected_for_case(&st.pool, case_id).await?;
    Ok(Json(
        json!({ "case_id": case_id, "expected_items_extracted": items.len(), "items": items }),
    ))
}

pub async fn brady_record_disclosed(
    State(st): State<AppState>,
    Json(body): Json<vi_brady_recon::DisclosedInput>,
) -> Result<Json<Value>, ApiError> {
    let id = vi_brady_recon::record_disclosed(&st.pool, &st.ledger, &body).await?;
    Ok(Json(json!({ "disclosed_item_id": id })))
}

pub async fn brady_reconcile(
    State(st): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let report = vi_brady_recon::reconcile::reconcile(&st.pool, &st.ledger, case_id).await?;
    Ok(Json(json!(report)))
}

pub async fn brady_report(
    State(st): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let row = sqlx::query_scalar::<_, Value>(
        "SELECT report FROM brady_recon_runs WHERE case_id=$1 ORDER BY run_at DESC LIMIT 1",
    )
    .bind(case_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let report: vi_brady_recon::reconcile::ReconReport = serde_json::from_value(row)?;
    let md = vi_brady_recon::report::render(&report).map_err(ApiError::internal)?;
    Ok((StatusCode::OK, [("content-type", "text/markdown")], md).into_response())
}

// ---------- Trial Penalty Observatory ----------

pub async fn tp_office(
    State(st): State<AppState>,
    Query(q): Query<OfficeQ>,
) -> Result<Json<Value>, ApiError> {
    let (dist, snap_id) = vi_trial_penalty::distribution::by_office(
        &st.pool,
        &st.ledger,
        &q.office,
        q.jurisdiction.as_deref(),
    )
    .await?;
    Ok(Json(
        json!({ "snapshot_id": snap_id, "distribution": dist }),
    ))
}

#[derive(Deserialize)]
pub struct JudgeQ {
    pub judge: String,
    pub jurisdiction: Option<String>,
}

pub async fn tp_judge(
    State(st): State<AppState>,
    Query(q): Query<JudgeQ>,
) -> Result<Json<Value>, ApiError> {
    let (dist, snap_id) = vi_trial_penalty::distribution::by_judge(
        &st.pool,
        &st.ledger,
        &q.judge,
        q.jurisdiction.as_deref(),
    )
    .await?;
    Ok(Json(
        json!({ "snapshot_id": snap_id, "distribution": dist }),
    ))
}

#[derive(Deserialize)]
pub struct HeatQ {
    pub resolution: Option<u8>,
    pub jurisdiction: Option<String>,
}

pub async fn tp_heatmap(
    State(st): State<AppState>,
    Query(q): Query<HeatQ>,
) -> Result<Json<Value>, ApiError> {
    let res = q.resolution.unwrap_or(7).clamp(3, 9);
    let cells =
        vi_trial_penalty::distribution::heatmap(&st.pool, res, q.jurisdiction.as_deref()).await?;
    Ok(Json(json!({ "resolution": res, "cells": cells })))
}

#[derive(Deserialize)]
pub struct DisparityQ {
    pub group_a: String,
    pub group_b: String,
    pub charge_category: Option<String>,
    pub threshold: Option<f64>,
}

pub async fn tp_disparity(
    State(st): State<AppState>,
    Query(q): Query<DisparityQ>,
) -> Result<Json<Value>, ApiError> {
    let comp = vi_trial_penalty::disparity::racial_disparity(
        &st.pool,
        q.charge_category.as_deref(),
        &q.group_a,
        &q.group_b,
        q.threshold.unwrap_or(2.0),
    )
    .await?;
    Ok(Json(json!(comp)))
}

pub async fn tp_motion(
    State(st): State<AppState>,
    Query(q): Query<OfficeQ>,
) -> Result<Response, ApiError> {
    let (dist, _) = vi_trial_penalty::distribution::by_office(
        &st.pool,
        &st.ledger,
        &q.office,
        q.jurisdiction.as_deref(),
    )
    .await?;
    let ctx = vi_trial_penalty::motion::MotionContext::from_distribution(q.office.clone(), &dist);
    let md = vi_trial_penalty::motion::render(&ctx).map_err(ApiError::internal)?;
    Ok((StatusCode::OK, [("content-type", "text/markdown")], md).into_response())
}

#[cfg(test)]
mod tests {
    use super::unit_interval;

    #[test]
    fn unit_interval_bounds() {
        assert!(unit_interval(0.0, "x").is_ok());
        assert!(unit_interval(1.0, "x").is_ok());
        assert!(unit_interval(1.1, "x").is_err());
        assert!(unit_interval(-0.1, "x").is_err());
        assert!(unit_interval(f64::NAN, "x").is_err());
    }
}
