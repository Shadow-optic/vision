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
    let _ = vi_geo::k_ring(&cell, 0)?;
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

    let ladder: Vec<Value> = sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
             'resolution', resolution, 'cell', h3_cell, 'cases', COUNT(DISTINCT case_id))
           FROM case_h3_cells
           WHERE case_id IN (
             SELECT case_id FROM court_cases WHERE court_h3_cell = $1 OR incident_h3_cell = $1
             UNION
             SELECT case_id FROM case_h3_cells WHERE h3_cell = $1
           )
           GROUP BY resolution, h3_cell
           ORDER BY resolution"#,
    )
    .bind(&cell)
    .fetch_all(&st.pool)
    .await?;

    let boundary = vi_geo::cell_boundary(&cell)?;
    let mut obj = row;
    if let Some(map) = obj.as_object_mut() {
        map.insert("ladder".into(), json!(ladder));
        map.insert(
            "boundary".into(),
            json!(boundary
                .iter()
                .map(|(lat, lng)| json!({"lat": lat, "lng": lng}))
                .collect::<Vec<_>>()),
        );
    }
    Ok(Json(obj))
}

#[derive(Deserialize)]
pub struct KringQ {
    pub k: Option<u32>,
}

#[derive(sqlx::FromRow)]
struct CellAgg {
    cell: Option<String>,
    total: i64,
    convictions: i64,
    avg_sentence_months: Option<f64>,
}

pub async fn kring_stats(
    State(st): State<AppState>,
    Path(cell): Path<String>,
    Query(q): Query<KringQ>,
) -> Result<Json<Value>, ApiError> {
    let k = q.k.unwrap_or(1).min(5);
    let neighbors = vi_geo::k_ring(&cell, k)?;
    let rows: Vec<CellAgg> = sqlx::query_as(
        r#"SELECT cell,
                  COUNT(*)::bigint AS total,
                  COUNT(*) FILTER (WHERE outcome = 'conviction')::bigint AS convictions,
                  AVG(sentence_months::float8) FILTER (WHERE outcome = 'conviction') AS avg_sentence_months
           FROM (
             SELECT court_h3_cell AS cell, outcome, sentence_months
             FROM court_cases WHERE court_h3_cell = ANY($1)
             UNION ALL
             SELECT incident_h3_cell, outcome, sentence_months
             FROM court_cases
             WHERE incident_h3_cell = ANY($1)
               AND incident_h3_cell IS DISTINCT FROM court_h3_cell
           ) t
           GROUP BY cell"#,
    )
    .bind(&neighbors)
    .fetch_all(&st.pool)
    .await?;

    let by_cell: std::collections::HashMap<String, CellAgg> = rows
        .into_iter()
        .filter_map(|r| r.cell.clone().map(|c| (c, r)))
        .collect();

    let cells: Vec<Value> = neighbors
        .iter()
        .map(|n| {
            let agg = by_cell.get(n);
            let total = agg.map(|a| a.total).unwrap_or(0);
            let convictions = agg.map(|a| a.convictions).unwrap_or(0);
            json!({
                "cell": n,
                "total": total,
                "convictions": convictions,
                "conviction_rate": if total > 0 {
                    Some(convictions as f64 / total as f64)
                } else {
                    None
                },
                "avg_sentence_months": agg.and_then(|a| a.avg_sentence_months),
                "origin": n == &cell,
            })
        })
        .collect();

    Ok(Json(json!({
        "origin": cell,
        "k": k,
        "cells": cells,
    })))
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
    // Same code path the ingestion pipeline uses, so an operator's request and
    // a scheduled cycle can never drift apart.
    let fired = vi_pipeline::run_rules_for_case(&st.pool, &st.ledger, body.case_id).await?;
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
    validate_sim_inputs(&req.priors, &req.strategy)?;
    let trials = req.trials.clamp(100, 1_000_000);
    let seed = req.seed.unwrap_or(42);
    let (sim_id, entry, dist) =
        persist_simulation(&st, req.priors, req.strategy, trials, seed).await?;
    Ok(Json(json!({
        "simulation_id": sim_id,
        "ledger_seq": entry.seq,
        "priors_source": "caller",
        "distribution": dist
    })))
}

fn validate_sim_inputs(p: &vi_sim::CasePriors, s: &vi_sim::Strategy) -> Result<(), ApiError> {
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
    Ok(())
}

async fn persist_simulation(
    st: &AppState,
    priors: vi_sim::CasePriors,
    strategy: vi_sim::Strategy,
    trials: u32,
    seed: u64,
) -> Result<(Uuid, vi_ledger::LedgerEntry, vi_sim::Distribution), ApiError> {
    let (p2, s2) = (priors.clone(), strategy.clone());
    let dist = tokio::task::spawn_blocking(move || vi_sim::simulate(&p2, &s2, trials, seed))
        .await
        .map_err(ApiError::internal)?;

    let sim_id = Uuid::new_v4();
    let params = json!({"priors": priors, "strategy": strategy, "trials": trials, "seed": seed});
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
    Ok((sim_id, entry, dist))
}

#[derive(Deserialize, Default)]
pub struct SimFromCase {
    pub strategy: Option<vi_sim::Strategy>,
    pub trials: Option<u32>,
    pub seed: Option<u64>,
}

#[derive(sqlx::FromRow)]
struct CaseCalib {
    evidence_strength: Option<String>,
    charge_category: Option<String>,
    plea_offer_months: Option<i32>,
    sentence_months: Option<i32>,
    judge: Option<String>,
    office: Option<String>,
}

pub async fn simulate_from_case(
    State(st): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(req): Json<SimFromCase>,
) -> Result<Json<Value>, ApiError> {
    let row = sqlx::query_as::<_, CaseCalib>(
        "SELECT c.evidence_strength, c.charge_category, c.plea_offer_months, c.sentence_months,
                c.judge, p.office
         FROM court_cases c
         LEFT JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
         WHERE c.case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;

    let (office_conviction, mean_plea, mean_trial): (Option<f64>, Option<f64>, Option<f64>) =
        if let Some(office) = row.office.as_deref() {
            sqlx::query_as(
                "SELECT
                   (COUNT(*) FILTER (WHERE outcome = 'conviction')::float8
                     / NULLIF(COUNT(*) FILTER (WHERE outcome IS NOT NULL), 0)) AS conviction_rate,
                   AVG(sentence_months::float8) FILTER (WHERE plea_accepted) AS mean_plea,
                   AVG(sentence_months::float8) FILTER (WHERE NOT plea_accepted AND outcome = 'conviction') AS mean_trial
                 FROM court_cases c
                 JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
                 WHERE p.office = $1",
            )
            .bind(office)
            .fetch_one(&st.pool)
            .await?
        } else {
            (None, None, None)
        };

    let judge_rate: Option<f64> = if let Some(judge) = row.judge.as_deref() {
        sqlx::query_scalar(
            "SELECT COUNT(*) FILTER (WHERE outcome = 'conviction')::float8
               / NULLIF(COUNT(*) FILTER (WHERE outcome IS NOT NULL), 0)
             FROM court_cases WHERE judge = $1",
        )
        .bind(judge)
        .fetch_one(&st.pool)
        .await?
    } else {
        None
    };

    let (priors, priors_source) = vi_sim::priors_from_stats(&vi_sim::CalibrationInputs {
        evidence_strength: row.evidence_strength,
        charge_category: row.charge_category,
        office_conviction_rate: office_conviction,
        office_mean_plea_months: mean_plea,
        office_mean_trial_months: mean_trial,
        judge_conviction_rate: judge_rate,
        case_plea_offer_months: row.plea_offer_months.map(|n| n as f64),
        case_sentence_months: row.sentence_months.map(|n| n as f64),
    });

    let strategy = req.strategy.unwrap_or(vi_sim::Strategy {
        name: "calibrated-baseline".into(),
        plea_discount: 0.10,
        suppression_bonus: 0.05,
        acquittal_bonus: 0.05,
    });
    validate_sim_inputs(&priors, &strategy)?;
    let trials = req.trials.unwrap_or(10_000).clamp(100, 1_000_000);
    let seed = req.seed.unwrap_or(42);
    let (sim_id, entry, dist) =
        persist_simulation(&st, priors.clone(), strategy.clone(), trials, seed).await?;
    Ok(Json(json!({
        "simulation_id": sim_id,
        "ledger_seq": entry.seq,
        "case_id": case_id,
        "priors_source": priors_source,
        "priors": priors,
        "strategy": strategy,
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
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT COALESCE(c.docket_number, c.case_id::text),
                (SELECT o.citation FROM court_opinions o WHERE o.case_id = c.case_id LIMIT 1),
                p.office
         FROM court_cases c
         LEFT JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
         WHERE c.case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(&st.pool)
    .await?;
    let (docket, citation, office) = row.ok_or_else(ApiError::not_found)?;
    let caption = citation.clone().unwrap_or_else(|| docket.clone());

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

    let brady_gaps = match sqlx::query_scalar::<_, Value>(
        "SELECT report FROM brady_recon_runs WHERE case_id=$1 ORDER BY run_at DESC LIMIT 1",
    )
    .bind(case_id)
    .fetch_optional(&st.pool)
    .await?
    {
        Some(report) => report
            .get("gaps")
            .and_then(Value::as_array)
            .map(|gaps| {
                gaps.iter()
                    .filter_map(|g| {
                        Some(vi_lasm::BradyGap {
                            item_type: g.get("item_type")?.as_str()?.to_string(),
                            description: g.get("description")?.as_str()?.to_string(),
                            source_reference: g
                                .get("source_reference")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        None => Vec::new(),
    };

    let monell = if let Some(office) = office.as_deref() {
        let fp = vi_monell_atlas::stats::office_fingerprint(&st.pool, office, None).await?;
        Some(vi_lasm::MonellSummary {
            office: fp.office,
            total_substantiated: fp.total_substantiated,
            interpretation: fp.interpretation,
        })
    } else {
        None
    };

    let trial_penalty = if let Some(office) = office.as_deref() {
        let (n, mean_ratio): (i64, Option<f64>) = sqlx::query_as(
            "SELECT COUNT(*)::bigint,
                    AVG(cc.sentence_months::float / cc.plea_offer_months)
             FROM court_cases cc
             JOIN prosecutors p ON p.prosecutor_id = cc.prosecutor_id
             WHERE p.office = $1
               AND cc.plea_offered = true
               AND cc.plea_accepted = false
               AND cc.outcome = 'conviction'
               AND cc.plea_offer_months > 0
               AND cc.sentence_months > 0",
        )
        .bind(office)
        .fetch_one(&st.pool)
        .await?;
        Some(vi_lasm::TrialPenaltySummary {
            office: office.to_string(),
            n,
            mean_ratio,
        })
    } else {
        None
    };

    let constitution = sqlx::query_as::<_, (String, i32, String)>(
        "SELECT jurisdiction, hit_count, COALESCE(report->>'authority', 'advisory')
         FROM constitution_screens WHERE case_id=$1
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(case_id)
    .fetch_optional(&st.pool)
    .await?
    .map(
        |(jurisdiction, hit_count, authority)| vi_lasm::ConstitutionSummary {
            jurisdiction,
            hit_count: hit_count as i64,
            authority,
        },
    );

    let md = vi_lasm::render(&vi_lasm::EvidencePackage {
        caption,
        docket_number: docket,
        flags: summaries,
        ledger,
        brady_gaps,
        monell,
        trial_penalty,
        constitution,
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

// ---------- Tactics ----------

#[derive(Deserialize)]
pub struct TacticQ {
    pub category: Option<String>,
}

pub async fn list_tactics(
    State(st): State<AppState>,
    Query(q): Query<TacticQ>,
) -> Result<Json<Value>, ApiError> {
    let tactics = vi_tactics::list(&st.pool, q.category.as_deref()).await?;
    Ok(Json(json!({ "tactics": tactics })))
}

pub async fn get_tactic(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(vi_tactics::get(&st.pool, id).await?)))
}

pub async fn create_tactic(
    State(st): State<AppState>,
    Json(body): Json<vi_tactics::NewTactic>,
) -> Result<Json<Value>, ApiError> {
    let id = vi_tactics::create(&st.pool, &st.ledger, &body).await?;
    Ok(Json(json!({ "tactic_id": id })))
}

pub async fn tactic_stats(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(vi_tactics::refresh_stats(&st.pool, id).await?)))
}

// ---------- Ingest ----------

#[derive(Deserialize)]
pub struct IngestRun {
    pub source: String,
}

pub async fn ingest_run(
    State(st): State<AppState>,
    Json(body): Json<IngestRun>,
) -> Result<Json<Value>, ApiError> {
    let runs = vi_ingest::run_named(&st.pool, &st.ledger, &body.source).await?;
    Ok(Json(json!({
        "source": body.source,
        "runs": runs,
        "totals": {
            "cases": runs.iter().map(|r| r.cases_persisted).sum::<u64>(),
            "opinions": runs.iter().map(|r| r.opinions_persisted).sum::<u64>(),
            "courts": runs.iter().map(|r| r.courts_persisted).sum::<u64>(),
            "skipped": runs.iter().map(|r| r.skipped).sum::<u64>(),
        },
    })))
}

pub async fn ingest_status(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let cursors = vi_ingest::list_cursors(&st.pool).await?;
    Ok(Json(json!({ "cursors": cursors })))
}

/// The feeds this deployment reads, whether each one has ever polled, and what
/// ingestion refused to store.
pub async fn ingest_sources(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(vi_ingest::list_sources(&st.pool).await?))
}

/// Place cases whose court could not be resolved when they were ingested.
pub async fn ingest_place_courts(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(vi_ingest::backfill_unplaced_courts(&st.pool).await?))
}

#[derive(Deserialize)]
pub struct PipelineRun {
    /// One case, or every case still awaiting the pipeline.
    pub case_id: Option<Uuid>,
    pub limit: Option<i64>,
}

/// Walk ingested records through every engine. Produces pending artifacts only.
pub async fn pipeline_run(
    State(st): State<AppState>,
    Json(body): Json<PipelineRun>,
) -> Result<Json<Value>, ApiError> {
    let summary = match body.case_id {
        Some(case_id) => {
            vi_pipeline::run_cases(&st.pool, &st.ledger, &[case_id], "operator").await?
        }
        None => {
            let limit = body.limit.unwrap_or(200).clamp(1, 2000);
            vi_pipeline::run_pending(&st.pool, &st.ledger, limit, "operator").await?
        }
    };
    Ok(Json(json!(summary)))
}

pub async fn pipeline_status(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(vi_pipeline::status(&st.pool).await?))
}

#[derive(Deserialize)]
pub struct UnresolvedQuery {
    #[serde(default)]
    pub include_resolved: bool,
}

/// Judge and counsel fields the pipeline refused to guess at.
pub async fn pipeline_unresolved(
    State(st): State<AppState>,
    Query(q): Query<UnresolvedQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        vi_pipeline::unresolved_officials(&st.pool, q.include_resolved).await?,
    ))
}

// ---------- Engine catalog ----------

pub async fn engines(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    vi_db::ping(&st.pool).await?;
    let cases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM court_cases")
        .fetch_one(&st.pool)
        .await?;
    let opinions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM court_opinions")
        .fetch_one(&st.pool)
        .await?;
    let tactics: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tactics")
        .fetch_one(&st.pool)
        .await?;
    let rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM abuse_rules")
        .fetch_one(&st.pool)
        .await?;
    let findings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM constitutional_findings")
        .fetch_one(&st.pool)
        .await?;
    let ledger_n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(&st.pool)
        .await?;
    let sims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM simulations")
        .fetch_one(&st.pool)
        .await?;
    let h3: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM case_h3_cells")
        .fetch_one(&st.pool)
        .await?;

    let provisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM constitution_provisions")
        .fetch_one(&st.pool)
        .await?;
    let actors: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accountability_actors")
        .fetch_one(&st.pool)
        .await?;
    let pipeline_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_runs")
        .fetch_one(&st.pool)
        .await?;

    Ok(Json(json!({
        "backend": "vi-api",
        "database": "ready",
        "engines": [
            {"name": "Root Ledger", "crate": "vi-ledger", "rows": ledger_n,
             "routes": ["/ledger/verify"]},
            {"name": "Case-law DB", "crate": "vi-api", "rows": opinions,
             "routes": ["/cases/search", "/cases/:id"]},
            {"name": "Correlation", "crate": "vi-correlation", "rows": cases,
             "routes": ["/stats/pearson", "/stats/odds", "/stats/plea-sentence"]},
            {"name": "Tactics DB", "crate": "vi-tactics", "rows": tactics,
             "routes": ["/tactics", "/tactics/:id", "/tactics/:id/stats"]},
            {"name": "Abuse detection", "crate": "vi-trustscript", "rows": rules,
             "routes": ["/rules", "/rules/run", "/flags"]},
            {"name": "Zero-day sim", "crate": "vi-sim", "rows": sims,
             "routes": ["/simulate", "/simulate/from-case/:case_id"]},
            {"name": "H3 intelligence", "crate": "vi-geo", "rows": h3,
             "routes": ["/geo/cells/:cell", "/geo/kring/:cell"]},
            {"name": "Telemetry / ingest", "crate": "vi-ingest", "rows": cases,
             "routes": ["/ingest/run", "/ingest/status", "/ingest/sources"]},
            {"name": "Post-ingest pipeline", "crate": "vi-pipeline", "rows": pipeline_runs,
             "routes": ["/pipeline/run", "/pipeline/status"]},
            {"name": "JIT LASM", "crate": "vi-lasm", "rows": cases,
             "routes": ["/lasm/package/:case_id"]},
            {"name": "Monell atlas", "crate": "vi-monell-atlas", "rows": findings,
             "routes": ["/atlas/findings", "/atlas/offices/fingerprint", "/atlas/offices/monell-report"]},
            {"name": "Brady recon", "crate": "vi-brady-recon", "rows": cases,
             "routes": ["/brady/derive/:case_id", "/brady/reconcile/:case_id", "/brady/lead-report/:case_id"]},
            {"name": "Trial penalty", "crate": "vi-trial-penalty", "rows": cases,
             "routes": ["/trial-penalty/offices", "/trial-penalty/heatmap", "/trial-penalty/disparity", "/trial-penalty/motion"]},
            {"name": "Constitution / Bill of Rights", "crate": "vi-constitution", "rows": provisions,
             "routes": ["/constitution", "/constitution/options", "/constitution/jurisdictions", "/constitution/provisions", "/constitution/resolve", "/constitution/screen/:case_id"]},
            {"name": "Reckoning / individual accountability", "crate": "vi-reckoning", "rows": actors,
             "routes": ["/reckoning/actors", "/reckoning/resolve", "/reckoning/actors/:id/package", "/reckoning/wall", "/reckoning/wall/:id", "/reckoning/statutes"]}
        ]
    })))
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
