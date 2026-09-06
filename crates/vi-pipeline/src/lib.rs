//! What happens to a record after it is ingested.
//!
//! Ingestion stores public records. This crate is the wiring that walks each
//! new record through the engines that have something to say about it:
//!
//! 1. `forum` — is the case in a jurisdiction the corpus knows? Screening a
//!    case under the wrong body of law is worse than not screening it.
//! 2. `constitution_screen` — which clauses the fact pattern touches, resolved
//!    against controlling authority.
//! 3. `evidence_leads` — expected evidence derived from the opinion text, then
//!    reconciled against what was disclosed. Gaps are leads, not findings.
//! 4. `abuse_rules` — enabled TrustScript rules, producing pending flags.
//! 5. `actor_links` — the individuals named in the record, resolved to stable
//!    identities and linked to the case.
//! 6. `score` — recompute each linked individual's Abuse Score.
//! 7. `tactics_occurrences` — match the record's text and charge list against
//!    the tactic catalog, recording occurrences as pending leads.
//! 8. `trial_penalty` — fold the case's disposition fields into office-level
//!    distributions when they exist; counted as skipped-no-data when they do
//!    not (most live feeds carry no disposition data).
//! 9. `monell_refresh` — recompute and store the Monell fingerprint for the
//!    case's office, when the case is attributed to one.
//! 10. `sim_calibration` — refresh the stored office statistics the zero-day
//!    sim calibrates from, when the case is attributed to an office.
//! 11. `correlation_refresh` — refresh the office's plea-offer/sentence
//!    correlation from accumulated records; reported as absent rather than
//!    filled in when there are too few paired observations.
//! 12. `drift_ingest_signals` — fold the case's screened opinions into the
//!    per-(court, clause) outcome-signal series the drift engine detects over.
//! 13. `drift_detect` — re-run changepoint detection for every (court, clause)
//!    series this case contributes observations to.
//! 14. `capture_rebuild` — rebuild the judge x court x outcome edge table so
//!    the new opinions are part of the capture graph.
//! 15. `capture_metrics` — recompute concentration metrics against the seeded
//!    Monte Carlo null (1000 permutations, fixed seed, reproducible).
//! 16. `resonance_compute` — weak-signal fusion across all engines, LAST, so
//!    every signal producer above has already run for this case.
//!
//! Drift, capture, and resonance artifacts are machine-derived `pending`
//! leads, exactly like screens and flags. Transparency snapshots stay
//! on-demand (`POST /transparency/snapshot`), never per-cycle.
//!
//! Engines deliberately not in this list: vi-geo (runs at ingest and API
//! startup, not per pipeline case), vi-sim's Monte Carlo itself (on demand),
//! vi-lasm (on-demand package assembly), `/reckoning/sync` (an operator
//! action, not a per-case step), and vi-transparency (on-demand snapshots).
//!
//! Every artifact this pipeline creates is pending by construction. Screens,
//! leads, flags, occurrences, links, and scores publish nothing and accuse no
//! one: the Abuse Score counts only counsel-substantiated material, so a case
//! that has just been ingested moves nobody's score off zero. Publication
//! remains a separate, human, licensed-counsel act.
#![forbid(unsafe_code)]

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;
use vi_ledger::Ledger;

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("tactics: {0}")]
    Tactics(#[from] vi_tactics::Error),
    #[error("trial penalty: {0}")]
    TrialPenalty(#[from] vi_trial_penalty::Error),
    #[error("drift: {0}")]
    Drift(#[from] vi_drift::Error),
    #[error("capture: {0}")]
    Capture(#[from] vi_capture::Error),
    #[error("resonance: {0}")]
    Resonance(#[from] vi_resonance::Error),
    #[error("reckoning: {0}")]
    Reckoning(#[from] vi_reckoning::Error),
    #[error("case not found")]
    NotFound,
    #[error("flag not found")]
    FlagNotFound,
    #[error("invalid review status: {0}")]
    InvalidStatus(String),
    #[error("invalid resolution: {0}")]
    InvalidResolution(String),
    #[error("unresolved-officials entry not found")]
    UnresolvedNotFound,
    #[error("unresolved-officials entry is already resolved")]
    AlreadyResolved,
}

/// The stages every case is walked through, in order. Kept as data so the
/// per-run report can count a stage that ran zero times instead of dropping
/// it silently.
pub const STAGES: &[&str] = &[
    "forum",
    "constitution_screen",
    "evidence_leads",
    "abuse_rules",
    "actor_links",
    "score",
    "tactics_occurrences",
    "trial_penalty",
    "monell_refresh",
    "sim_calibration",
    "correlation_refresh",
    "drift_ingest_signals",
    "drift_detect",
    "capture_rebuild",
    "capture_metrics",
    "resonance_compute",
];

/// Fixed seed for the pipeline's capture-metric null model, so repeated
/// pipeline runs are reproducible and the ledger events are comparable.
pub const CAPTURE_NULL_SEED: u64 = 0xC4A7_0E15;

/// Hazard (expected segment length) for drift detection triggered by the
/// pipeline; conservative, matches the API route default.
pub const DRIFT_DEFAULT_HAZARD: f64 = 50.0;

/// Result of one stage. A stage that cannot run says so and why; it never
/// pretends to have succeeded.
#[derive(Debug, Clone, Serialize)]
pub struct StageOutcome {
    pub stage: &'static str,
    /// `ok`, `skipped`, or `failed`.
    pub status: &'static str,
    pub detail: Value,
}

impl StageOutcome {
    fn ok(stage: &'static str, detail: Value) -> Self {
        Self {
            stage,
            status: "ok",
            detail,
        }
    }

    fn skipped(stage: &'static str, reason: &str) -> Self {
        Self {
            stage,
            status: "skipped",
            detail: json!({ "reason": reason }),
        }
    }

    fn failed(stage: &'static str, error: &str) -> Self {
        Self {
            stage,
            status: "failed",
            detail: json!({ "error": error }),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseRun {
    pub run_id: Uuid,
    pub case_id: Uuid,
    pub docket_number: Option<String>,
    pub trigger: String,
    /// `ok`, `partial`, or `failed`.
    pub status: &'static str,
    pub stages: Vec<StageOutcome>,
    pub screen_id: Option<Uuid>,
    pub screen_hits: i32,
    pub flags_fired: i32,
    pub expected_items: i32,
    pub evidence_gaps: i32,
    pub actors_linked: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub cases_processed: usize,
    pub ok: usize,
    pub partial: usize,
    pub failed: usize,
    pub screens: usize,
    pub flags_fired: i64,
    pub evidence_gaps: i64,
    pub actors_linked: i64,
    /// Per-stage counts across the batch: how many cases each stage processed,
    /// skipped for lack of data, or failed on. A stage that had nothing to
    /// work with shows up here as skipped, not as silence.
    pub stage_counts: Value,
    /// The `pipeline_reports` row persisted for this batch.
    pub report_id: Option<Uuid>,
    pub runs: Vec<CaseRun>,
}

/// Cases with no completed pipeline run yet, oldest first.
pub async fn pending_case_ids(pool: &PgPool, limit: i64) -> Result<Vec<Uuid>, Error> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT c.case_id FROM court_cases c
          WHERE NOT EXISTS (
                SELECT 1 FROM pipeline_runs r
                 WHERE r.case_id = c.case_id AND r.status IN ('ok', 'partial'))
          ORDER BY c.inserted_at
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Process every case that has not been through the pipeline yet.
pub async fn run_pending(
    pool: &PgPool,
    ledger: &Ledger,
    limit: i64,
    trigger: &str,
) -> Result<RunSummary, Error> {
    let ids = pending_case_ids(pool, limit).await?;
    run_cases(pool, ledger, &ids, trigger).await
}

pub async fn run_cases(
    pool: &PgPool,
    ledger: &Ledger,
    case_ids: &[Uuid],
    trigger: &str,
) -> Result<RunSummary, Error> {
    let mut summary = RunSummary {
        cases_processed: 0,
        ok: 0,
        partial: 0,
        failed: 0,
        screens: 0,
        flags_fired: 0,
        evidence_gaps: 0,
        actors_linked: 0,
        stage_counts: json!({}),
        report_id: None,
        runs: Vec::new(),
    };

    for case_id in case_ids {
        let run = match run_case(pool, ledger, *case_id, trigger).await {
            Ok(run) => run,
            Err(e) => {
                // A single unprocessable case must not stop the queue.
                tracing::error!(case_id = %case_id, error = %e, "pipeline run failed");
                continue;
            }
        };
        summary.cases_processed += 1;
        match run.status {
            "ok" => summary.ok += 1,
            "partial" => summary.partial += 1,
            _ => summary.failed += 1,
        }
        if run.screen_id.is_some() {
            summary.screens += 1;
        }
        summary.flags_fired += i64::from(run.flags_fired);
        summary.evidence_gaps += i64::from(run.evidence_gaps);
        summary.actors_linked += i64::from(run.actors_linked);
        summary.runs.push(run);
    }

    // Per-run report: for every stage, how many cases it processed, how many
    // it skipped for lack of data, and how many it failed on. Stages are
    // counted under their canonical names even when no case reached them, so
    // a stage that silently stopped running reads as zeros, not absence.
    let mut counts = serde_json::Map::new();
    for stage in STAGES {
        counts.insert(
            stage.to_string(),
            json!({ "processed": 0, "skipped_no_data": 0, "failed": 0 }),
        );
    }
    for run in &summary.runs {
        for stage in &run.stages {
            let entry = counts
                .entry(stage.stage.to_string())
                .or_insert_with(|| json!({ "processed": 0, "skipped_no_data": 0, "failed": 0 }));
            let key = match stage.status {
                "ok" => "processed",
                "skipped" => "skipped_no_data",
                _ => "failed",
            };
            if let Some(v) = entry.get(key).and_then(Value::as_i64) {
                entry[key] = json!(v + 1);
            }
        }
    }
    summary.stage_counts = Value::Object(counts);

    let report_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO pipeline_reports (trigger, cases_processed, stage_counts)
         VALUES ($1,$2,$3) RETURNING report_id",
    )
    .bind(trigger)
    .bind(summary.cases_processed as i32)
    .bind(&summary.stage_counts)
    .fetch_one(pool)
    .await?;
    summary.report_id = Some(report_id);

    Ok(summary)
}

#[derive(Debug, sqlx::FromRow)]
struct CaseHeader {
    docket_number: Option<String>,
    jurisdiction: String,
    court_level: Option<String>,
    judge: Option<String>,
    prosecutor_id: Option<Uuid>,
    office: Option<String>,
    opinion_count: i64,
    partial_text: i64,
}

/// Walk one case through every stage.
pub async fn run_case(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
    trigger: &str,
) -> Result<CaseRun, Error> {
    let header = sqlx::query_as::<_, CaseHeader>(
        "SELECT c.docket_number, c.jurisdiction, c.court_level, c.judge,
                c.prosecutor_id, p.office,
                (SELECT COUNT(*) FROM court_opinions o WHERE o.case_id = c.case_id)
                    AS opinion_count,
                (SELECT COUNT(*) FROM court_opinions o
                  WHERE o.case_id = c.case_id AND o.text_completeness <> 'full')
                    AS partial_text
           FROM court_cases c
           LEFT JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
          WHERE c.case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound)?;

    let run_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO pipeline_runs (case_id, trigger) VALUES ($1,$2) RETURNING run_id",
    )
    .bind(case_id)
    .bind(trigger)
    .fetch_one(pool)
    .await?;

    let mut stages: Vec<StageOutcome> = Vec::new();
    let mut screen_id = None;
    let mut screen_hits = 0i32;
    let mut flags_fired = 0i32;
    let mut expected_items = 0i32;
    let mut evidence_gaps = 0i32;
    let mut actors_linked = 0i32;

    // --- 1. Forum ---------------------------------------------------------
    let forum = vi_constitution::jurisdictions::lookup(&header.jurisdiction);
    match forum {
        Some(j) => stages.push(StageOutcome::ok(
            "forum",
            json!({
                "jurisdiction": j.code,
                "name": j.name,
                "circuit": j.circuit,
                "court_level": header.court_level,
            }),
        )),
        None => stages.push(StageOutcome::skipped(
            "forum",
            &format!(
                "'{}' is not a known forum; the court registry has not placed this court yet",
                header.jurisdiction
            ),
        )),
    }

    // --- 2. Constitutional screen ----------------------------------------
    if forum.is_some() {
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT screen_id FROM constitution_screens WHERE case_id = $1
              ORDER BY created_at DESC LIMIT 1",
        )
        .bind(case_id)
        .fetch_optional(pool)
        .await?;
        match existing {
            Some(id) => {
                screen_id = Some(id);
                stages.push(StageOutcome::ok(
                    "constitution_screen",
                    json!({ "screen_id": id, "reused": true }),
                ));
            }
            None => match vi_constitution::db::screen_case(pool, ledger, case_id).await {
                Ok((id, report, _md)) => {
                    screen_id = Some(id);
                    screen_hits = report.hits.len() as i32;
                    stages.push(StageOutcome::ok(
                        "constitution_screen",
                        json!({
                            "screen_id": id,
                            "hits": screen_hits,
                            "clauses": report.hits.iter().map(|h| h.clause_id).collect::<Vec<_>>(),
                        }),
                    ));
                }
                Err(e) => stages.push(StageOutcome::failed("constitution_screen", &e.to_string())),
            },
        }
    } else {
        stages.push(StageOutcome::skipped(
            "constitution_screen",
            "forum unresolved",
        ));
    }

    // --- 3. Evidence leads ------------------------------------------------
    if header.opinion_count == 0 {
        stages.push(StageOutcome::skipped(
            "evidence_leads",
            "no opinion text for this case",
        ));
    } else {
        match vi_brady_recon::extractor::derive_expected_for_case(pool, case_id).await {
            Ok(items) => {
                expected_items = items.len() as i32;
                match vi_brady_recon::reconcile::reconcile(pool, ledger, case_id).await {
                    Ok(report) => {
                        evidence_gaps = report.gap_count as i32;
                        stages.push(StageOutcome::ok(
                            "evidence_leads",
                            json!({
                                "expected_items": expected_items,
                                "disclosed_items": report.disclosed_count,
                                "gaps": evidence_gaps,
                                "text_completeness": if header.partial_text > 0 {
                                    "partial"
                                } else {
                                    "full"
                                },
                                "caveat": if header.partial_text > 0 {
                                    "Derived from partial text. Absence of a mention is not \
                                     evidence that nothing exists."
                                } else {
                                    "Derived from complete opinion text."
                                },
                            }),
                        ));
                    }
                    Err(e) => stages.push(StageOutcome::failed("evidence_leads", &e.to_string())),
                }
            }
            Err(e) => stages.push(StageOutcome::failed("evidence_leads", &e.to_string())),
        }
    }

    // --- 4. Abuse rules ---------------------------------------------------
    match run_rules_for_case(pool, ledger, case_id).await {
        Ok(fired) => {
            flags_fired = fired.len() as i32;
            stages.push(StageOutcome::ok(
                "abuse_rules",
                json!({
                    "flags_fired": flags_fired,
                    "flags": fired,
                    "review_status": "pending",
                    "note": "A flag is a lead for counsel. It publishes nothing.",
                }),
            ));
        }
        Err(e) => stages.push(StageOutcome::failed("abuse_rules", &e.to_string())),
    }

    // --- 5. Actor links ---------------------------------------------------
    match link_actors(pool, ledger, case_id, &header).await {
        Ok(outcome) => {
            actors_linked = outcome.linked.len() as i32;
            stages.push(StageOutcome::ok(
                "actor_links",
                json!({
                    "linked": outcome.linked,
                    "unresolved": outcome.unresolved,
                    "note": "A field naming no readable individual is queued for a \
                             human rather than resolved by guess.",
                }),
            ));
        }
        Err(e) => stages.push(StageOutcome::failed("actor_links", &e.to_string())),
    }

    // --- 6. Score ---------------------------------------------------------
    let linked_ids =
        sqlx::query_scalar::<_, Uuid>("SELECT actor_id FROM actor_case_links WHERE case_id = $1")
            .bind(case_id)
            .fetch_all(pool)
            .await?;
    let mut scores = Vec::new();
    let mut score_failures = Vec::new();
    for actor_id in &linked_ids {
        match vi_reckoning::score_actor(pool, Some(ledger), *actor_id).await {
            Ok(score) => scores.push(json!({
                "actor_id": actor_id,
                "score": score.score,
                "substantiated_findings": score.substantiated_findings,
            })),
            Err(e) => score_failures.push(json!({ "actor_id": actor_id, "error": e.to_string() })),
        }
    }
    if score_failures.is_empty() {
        stages.push(StageOutcome::ok(
            "score",
            json!({
                "scored": scores,
                "note": "Only counsel-substantiated findings and flags contribute.",
            }),
        ));
    } else {
        stages.push(StageOutcome::failed(
            "score",
            &format!("{score_failures:?}"),
        ));
    }

    // --- 7. Tactic occurrences --------------------------------------------
    // Match the record against the tactic catalog. A case with no text and
    // no charge list has nothing to match against — that is a counted skip,
    // not a silent drop.
    let n_charges: i64 = sqlx::query_scalar(
        "SELECT COALESCE(cardinality(charges), 0)::bigint FROM court_cases WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await?;
    if header.opinion_count == 0 && n_charges == 0 {
        stages.push(StageOutcome::skipped(
            "tactics_occurrences",
            "no opinion text or charge list for this case",
        ));
    } else {
        match vi_tactics::match_case_occurrences(pool, ledger, case_id).await {
            Ok(occurrences) => stages.push(StageOutcome::ok(
                "tactics_occurrences",
                json!({
                    "occurrences_recorded": occurrences.len(),
                    "occurrences": occurrences,
                    "review_status": "pending",
                    "note": "An occurrence records that the public record mentions a \
                             doctrine. It accuses no one and publishes nothing.",
                }),
            )),
            Err(e) => stages.push(StageOutcome::failed(
                "tactics_occurrences",
                &e.to_string(),
            )),
        }
    }

    // --- 8. Trial-penalty accumulation -------------------------------------
    match vi_trial_penalty::distribution::accumulate_case(pool, ledger, case_id).await {
        Ok(vi_trial_penalty::distribution::Accumulation::Accumulated { office }) => {
            stages.push(StageOutcome::ok(
                "trial_penalty",
                json!({ "accumulated": true, "office": office }),
            ));
        }
        Ok(vi_trial_penalty::distribution::Accumulation::SkippedNoData { missing }) => {
            stages.push(StageOutcome::skipped(
                "trial_penalty",
                &format!(
                    "case carries no {} — most public feeds publish none",
                    missing.join(", ")
                ),
            ));
        }
        Err(e) => stages.push(StageOutcome::failed("trial_penalty", &e.to_string())),
    }

    // --- 9. Monell fingerprint refresh --------------------------------------
    match header.office.as_deref() {
        None => stages.push(StageOutcome::skipped(
            "monell_refresh",
            "case is not attributed to an office",
        )),
        Some(office) => {
            match vi_monell_atlas::stats::refresh_office_fingerprint(
                pool,
                office,
                Some(&header.jurisdiction),
            )
            .await
            {
                Ok(fp) => stages.push(StageOutcome::ok(
                    "monell_refresh",
                    json!({
                        "office": fp.office,
                        "total_substantiated": fp.total_substantiated,
                        "note": "Fingerprints count counsel-substantiated findings only.",
                    }),
                )),
                Err(e) => stages.push(StageOutcome::failed("monell_refresh", &e.to_string())),
            }
        }
    }

    // --- 10. Sim prior calibration refresh ----------------------------------
    match header.office.as_deref() {
        None => stages.push(StageOutcome::skipped(
            "sim_calibration",
            "case is not attributed to an office",
        )),
        Some(office) => match refresh_office_sim_stats(pool, office, &header.jurisdiction).await {
            Ok(detail) => stages.push(StageOutcome::ok("sim_calibration", detail)),
            Err(e) => stages.push(StageOutcome::failed("sim_calibration", &e.to_string())),
        },
    }

    // --- 11. Correlation refresh --------------------------------------------
    match header.office.as_deref() {
        None => stages.push(StageOutcome::skipped(
            "correlation_refresh",
            "case is not attributed to an office",
        )),
        Some(office) => {
            match refresh_office_correlation(pool, office, &header.jurisdiction).await {
                Ok(detail) => {
                    if detail.get("plea_sentence_r").map(|v| v.is_null()).unwrap_or(true)
                        && detail
                            .get("plea_sentence_n")
                            .and_then(Value::as_i64)
                            .unwrap_or(0)
                            == 0
                    {
                        stages.push(StageOutcome::skipped(
                            "correlation_refresh",
                            "office has no paired plea-offer/sentence observations",
                        ));
                    } else {
                        stages.push(StageOutcome::ok("correlation_refresh", detail));
                    }
                }
                Err(e) => stages.push(StageOutcome::failed("correlation_refresh", &e.to_string())),
            }
        }
    }

    // --- 12. Drift signal ingestion -----------------------------------------
    // Fold this case's screened opinions into the per-(court, clause) outcome
    // series. Corpus-wide but idempotent; a case without opinions contributes
    // nothing and says so.
    if header.opinion_count == 0 {
        stages.push(StageOutcome::skipped(
            "drift_ingest_signals",
            "no opinions to derive outcome signals from",
        ));
    } else {
        match vi_drift::ingest_signals(pool).await {
            Ok(report) => stages.push(StageOutcome::ok(
                "drift_ingest_signals",
                serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
            )),
            Err(e) => stages.push(StageOutcome::failed("drift_ingest_signals", &e.to_string())),
        }
    }

    // --- 13. Drift detection --------------------------------------------------
    // Detect over exactly the series this case feeds, never over series it
    // has nothing to do with.
    let drift_pairs: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT d.court_id, d.clause_id
           FROM drift_observations d
           JOIN court_opinions o
             ON d.source_ref = COALESCE(o.source_ref, 'opinion:' || o.opinion_id::text)
          WHERE o.case_id = $1
          ORDER BY d.court_id, d.clause_id",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await?;
    if drift_pairs.is_empty() {
        stages.push(StageOutcome::skipped(
            "drift_detect",
            "case contributes no drift observations (no screen-hit clauses with a \
             lexicon match)",
        ));
    } else {
        let mut detail = Vec::new();
        let mut failed = false;
        for (court_id, clause_id) in &drift_pairs {
            match vi_drift::detect(pool, court_id, clause_id, DRIFT_DEFAULT_HAZARD).await {
                Ok(cps) => detail.push(json!({
                    "court_id": court_id,
                    "clause_id": clause_id,
                    "changepoints": cps.len(),
                })),
                Err(e) => {
                    stages.push(StageOutcome::failed(
                        "drift_detect",
                        &format!("{court_id}/{clause_id}: {e}"),
                    ));
                    failed = true;
                    break;
                }
            }
        }
        if !failed {
            stages.push(StageOutcome::ok(
                "drift_detect",
                json!({ "series": detail, "status": "pending" }),
            ));
        }
    }

    // --- 14. Capture edge rebuild ---------------------------------------------
    if header.opinion_count == 0 {
        stages.push(StageOutcome::skipped(
            "capture_rebuild",
            "no opinions to rebuild the capture graph from",
        ));
    } else {
        match vi_capture::rebuild_edges(pool).await {
            Ok(report) => stages.push(StageOutcome::ok(
                "capture_rebuild",
                serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
            )),
            Err(e) => stages.push(StageOutcome::failed("capture_rebuild", &e.to_string())),
        }
    }

    // --- 15. Capture metrics ----------------------------------------------------
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM capture_edges")
        .fetch_one(pool)
        .await?;
    if edge_count == 0 {
        stages.push(StageOutcome::skipped(
            "capture_metrics",
            "no capture edges (no authored opinions with a lexicon outcome signal)",
        ));
    } else {
        match vi_capture::compute_metrics(pool, vi_capture::MIN_PERMUTATIONS, CAPTURE_NULL_SEED)
            .await
        {
            Ok(report) => stages.push(StageOutcome::ok(
                "capture_metrics",
                serde_json::to_value(&report).unwrap_or_else(|_| json!({})),
            )),
            Err(e) => stages.push(StageOutcome::failed("capture_metrics", &e.to_string())),
        }
    }

    // --- 16. Resonance — LAST, after every signal producer ----------------------
    match vi_resonance::compute_all(pool).await {
        Ok(report) => stages.push(StageOutcome::ok(
            "resonance_compute",
            json!({
                "scored": report.scored,
                "surfaced": report.surfaced,
                "surface_q": vi_resonance::SURFACE_Q,
                "status": "pending",
            }),
        )),
        Err(e) => stages.push(StageOutcome::failed("resonance_compute", &e.to_string())),
    }

    let failed = stages.iter().filter(|s| s.status == "failed").count();
    let status = if failed == 0 {
        "ok"
    } else if failed == stages.len() {
        "failed"
    } else {
        "partial"
    };

    sqlx::query(
        "UPDATE pipeline_runs SET
            status = $2, stages = $3, screen_id = $4, screen_hits = $5,
            flags_fired = $6, expected_items = $7, evidence_gaps = $8,
            actors_linked = $9, finished_at = now()
          WHERE run_id = $1",
    )
    .bind(run_id)
    .bind(status)
    .bind(json!(&stages))
    .bind(screen_id)
    .bind(screen_hits)
    .bind(flags_fired)
    .bind(expected_items)
    .bind(evidence_gaps)
    .bind(actors_linked)
    .execute(pool)
    .await?;

    ledger
        .append(
            vi_ledger::events::PIPELINE_RUN,
            &json!({
                "run_id": run_id,
                "case_id": case_id,
                "trigger": trigger,
                "status": status,
                "screen_id": screen_id,
                "flags_fired": flags_fired,
                "evidence_gaps": evidence_gaps,
                "actors_linked": actors_linked,
                "stages_hash": vi_ledger::hash_payload(&json!(&stages)),
            }),
        )
        .await?;

    Ok(CaseRun {
        run_id,
        case_id,
        docket_number: header.docket_number,
        trigger: trigger.to_string(),
        status,
        stages,
        screen_id,
        screen_hits,
        flags_fired,
        expected_items,
        evidence_gaps,
        actors_linked,
    })
}

/// Refresh the stored office statistics the zero-day sim calibrates from
/// (`/simulate/from-case` reads these). Counts and means come straight from
/// `court_cases`; an office with no outcomes stores NULLs, and the sim then
/// says `priors_source: "fallback"` rather than inventing a rate.
async fn refresh_office_sim_stats(
    pool: &PgPool,
    office: &str,
    jurisdiction: &str,
) -> Result<Value, Error> {
    let row: (i64, Option<f64>, Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE c.outcome IS NOT NULL),
                (COUNT(*) FILTER (WHERE c.outcome = 'conviction')::float8
                  / NULLIF(COUNT(*) FILTER (WHERE c.outcome IS NOT NULL), 0)),
                AVG(c.sentence_months::float8) FILTER (WHERE c.plea_accepted),
                AVG(c.sentence_months::float8)
                  FILTER (WHERE NOT c.plea_accepted AND c.outcome = 'conviction')
           FROM court_cases c
           JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
          WHERE p.office = $1",
    )
    .bind(office)
    .fetch_one(pool)
    .await?;
    let (cases_with_outcome, conviction_rate, mean_plea, mean_trial) = row;

    sqlx::query(
        "INSERT INTO office_sim_stats
           (office, jurisdiction, cases_with_outcome, conviction_rate,
            mean_plea_months, mean_trial_months)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (office) DO UPDATE SET
            jurisdiction = EXCLUDED.jurisdiction,
            cases_with_outcome = EXCLUDED.cases_with_outcome,
            conviction_rate = EXCLUDED.conviction_rate,
            mean_plea_months = EXCLUDED.mean_plea_months,
            mean_trial_months = EXCLUDED.mean_trial_months,
            computed_at = now()",
    )
    .bind(office)
    .bind(jurisdiction)
    .bind(cases_with_outcome as i32)
    .bind(conviction_rate)
    .bind(mean_plea)
    .bind(mean_trial)
    .execute(pool)
    .await?;

    Ok(json!({
        "office": office,
        "cases_with_outcome": cases_with_outcome,
        "conviction_rate": conviction_rate,
        "mean_plea_months": mean_plea,
        "mean_trial_months": mean_trial,
        "note": if cases_with_outcome == 0 {
            "Office has no recorded outcomes yet; the sim will report its \
             fallback priors rather than a fabricated rate."
        } else {
            "Stored aggregates refreshed from public records."
        },
    }))
}

/// Refresh an office's plea-offer vs sentence-length correlation from the
/// accumulated public records (vi-correlation over office stats). Fewer than
/// 4 paired observations is reported as absent — a correlation over three
/// points is numerology, not a statistic.
async fn refresh_office_correlation(
    pool: &PgPool,
    office: &str,
    jurisdiction: &str,
) -> Result<Value, Error> {
    let rows: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT c.plea_offer_months, c.sentence_months
           FROM court_cases c
           JOIN prosecutors p ON p.prosecutor_id = c.prosecutor_id
          WHERE p.office = $1
            AND c.plea_offer_months IS NOT NULL
            AND c.sentence_months IS NOT NULL",
    )
    .bind(office)
    .fetch_all(pool)
    .await?;
    let x: Vec<f64> = rows.iter().map(|(a, _)| *a as f64).collect();
    let y: Vec<f64> = rows.iter().map(|(_, b)| *b as f64).collect();
    let r = vi_correlation::pearson(&x, &y).map(|res| res.r);

    sqlx::query(
        "INSERT INTO office_sim_stats (office, jurisdiction, plea_sentence_r, plea_sentence_n)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (office) DO UPDATE SET
            plea_sentence_r = EXCLUDED.plea_sentence_r,
            plea_sentence_n = EXCLUDED.plea_sentence_n,
            computed_at = now()",
    )
    .bind(office)
    .bind(jurisdiction)
    .bind(r)
    .bind(rows.len() as i32)
    .execute(pool)
    .await?;

    Ok(json!({
        "office": office,
        "plea_sentence_r": r,
        "plea_sentence_n": rows.len(),
        "note": if r.is_none() {
            "Fewer than 4 paired observations; no correlation is stored or shown."
        } else {
            "Pearson r over stored plea-offer/sentence pairs."
        },
    }))
}

/// Counsel review of an automated abuse flag: `substantiated` or `rejected`.
///
/// This is the gate between a machine lead and a public record. Substantiating
/// a flag recomputes the Abuse Score of every individual the flag is linked
/// to (scores count substantiated material only), so the score trail and the
/// ledger reflect the review the moment it happens.
pub async fn review_flag(
    pool: &PgPool,
    ledger: &Ledger,
    flag_id: Uuid,
    status: &str,
    notes: Option<&str>,
) -> Result<Value, Error> {
    if !matches!(status, "substantiated" | "rejected") {
        return Err(Error::InvalidStatus(status.to_string()));
    }

    let row: Option<(Uuid, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "UPDATE abuse_flags
            SET review_status = $1, reviewed_at = now(), review_notes = $3
          WHERE flag_id = $2
          RETURNING case_id, actor_id, prosecutor_id",
    )
    .bind(status)
    .bind(flag_id)
    .bind(notes)
    .fetch_optional(pool)
    .await?;
    let (case_id, flag_actor_id, flag_prosecutor_id) = row.ok_or(Error::FlagNotFound)?;

    ledger
        .append(
            vi_ledger::events::FLAG_REVIEWED,
            &json!({
                "flag_id": flag_id,
                "case_id": case_id,
                "status": status,
                "notes": notes,
            }),
        )
        .await?;

    // Substantiation changes what the Abuse Score counts for the individuals
    // this flag concerns: the linked actor, and any actor resolved to the
    // prosecutor record the flag names.
    let mut rescored = Vec::new();
    if status == "substantiated" {
        let mut actor_ids: Vec<Uuid> = flag_actor_id.into_iter().collect();
        if let Some(pid) = flag_prosecutor_id {
            let more = sqlx::query_scalar::<_, Uuid>(
                "SELECT actor_id FROM accountability_actors WHERE prosecutor_id = $1",
            )
            .bind(pid)
            .fetch_all(pool)
            .await?;
            for id in more {
                if !actor_ids.contains(&id) {
                    actor_ids.push(id);
                }
            }
        }
        for actor_id in actor_ids {
            let score = vi_reckoning::score_actor(pool, Some(ledger), actor_id).await?;
            rescored.push(json!({
                "actor_id": actor_id,
                "score": score.score,
                "snapshot_id": score.snapshot_id,
            }));
        }
    }

    Ok(json!({
        "flag_id": flag_id,
        "case_id": case_id,
        "review_status": status,
        "rescored_actors": rescored,
        "note": "A substantiated flag is counsel's finding from the public record; \
                 a pending or rejected flag feeds no score and no public page.",
    }))
}

/// Close an unresolved-officials queue entry.
///
/// `identified` means a human read the raw field and named the individuals
/// (resolution of those individuals into actors is a separate, deliberate
/// step via `/reckoning/resolve`); `not_identifiable` means the record does
/// not support naming anyone and the entry is closed as such. Either way the
/// close-out is ledger-chained.
pub async fn resolve_unresolved(
    pool: &PgPool,
    ledger: &Ledger,
    unresolved_id: Uuid,
    resolution: &str,
    notes: Option<&str>,
) -> Result<Value, Error> {
    if !matches!(resolution, "identified" | "not_identifiable") {
        return Err(Error::InvalidResolution(resolution.to_string()));
    }

    let current: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT case_id, resolved_at IS NOT NULL FROM unresolved_officials WHERE unresolved_id = $1",
    )
    .bind(unresolved_id)
    .fetch_optional(pool)
    .await?;
    let Some((case_id, already)) = current else {
        return Err(Error::UnresolvedNotFound);
    };
    if already {
        return Err(Error::AlreadyResolved);
    }

    sqlx::query(
        "UPDATE unresolved_officials
            SET resolved_at = now(), resolution = $2, resolution_notes = $3
          WHERE unresolved_id = $1",
    )
    .bind(unresolved_id)
    .bind(resolution)
    .bind(notes)
    .execute(pool)
    .await?;

    ledger
        .append(
            vi_ledger::events::OFFICIAL_RESOLVED,
            &json!({
                "unresolved_id": unresolved_id,
                "case_id": case_id,
                "resolution": resolution,
                "notes": notes,
            }),
        )
        .await?;

    Ok(json!({
        "unresolved_id": unresolved_id,
        "case_id": case_id,
        "resolution": resolution,
    }))
}

/// Evaluate every enabled rule against one case, recording pending flags.
///
/// Lives here rather than in the API layer so that scheduled ingestion and an
/// operator's request run exactly the same code.
pub async fn run_rules_for_case(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
) -> Result<Vec<Value>, Error> {
    let ctx = vi_db::case_context(pool, case_id)
        .await?
        .ok_or(Error::NotFound)?;

    let rules = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT rule_id, source FROM abuse_rules WHERE enabled",
    )
    .fetch_all(pool)
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
    for (rule_id, src) in rules {
        let Ok(rule) = vi_trustscript::parse_rule(&src) else {
            tracing::warn!(rule_id = %rule_id, "skipping a rule that no longer parses");
            continue;
        };
        let Some(flag) = vi_trustscript::evaluate(&rule, &ctx) else {
            continue;
        };

        // One pending flag per rule per case: re-running the pipeline must not
        // multiply the same lead.
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT flag_id FROM abuse_flags WHERE case_id = $1 AND rule_id = $2 LIMIT 1",
        )
        .bind(case_id)
        .bind(rule_id)
        .fetch_optional(pool)
        .await?;
        if let Some(flag_id) = existing {
            fired.push(json!({
                "flag_id": flag_id,
                "rule_id": rule_id,
                "label": flag.label,
                "existing": true,
            }));
            continue;
        }

        let flag_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO abuse_flags
               (case_id, rule_id, prosecutor_id, office, label, severity, explanation)
             VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING flag_id",
        )
        .bind(case_id)
        .bind(rule_id)
        .bind(prosecutor_id)
        .bind(&office)
        .bind(&flag.label)
        .bind(format!("{:?}", flag.severity).to_lowercase())
        .bind(json!(flag.matched))
        .fetch_one(pool)
        .await?;

        ledger
            .append(
                vi_ledger::events::ABUSE_FLAG,
                &json!({
                    "flag_id": flag_id,
                    "case_id": case_id,
                    "rule_id": rule_id,
                    "label": flag.label,
                    "explanation_hash": vi_ledger::hash_payload(&json!(flag.matched)),
                }),
            )
            .await?;

        fired.push(json!({
            "flag_id": flag_id,
            "rule_id": rule_id,
            "label": flag.label,
            "severity": format!("{:?}", flag.severity).to_lowercase(),
            "review_status": "pending",
        }));
    }
    Ok(fired)
}

/// Resolve the individuals a record names, link them to the case, and attach
/// the case's pending flags to them.
///
/// Only public-record identity fields are used. Resolution creates an identity;
/// it does not create an allegation.
/// Individuals a record named, and fields that named none readably.
#[derive(Debug, Default)]
struct ActorLinks {
    linked: Vec<Value>,
    unresolved: Vec<Value>,
}

async fn link_actors(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
    header: &CaseHeader,
) -> Result<ActorLinks, Error> {
    let mut out = ActorLinks::default();

    if let (Some(pid), Some(office)) = (header.prosecutor_id, header.office.clone()) {
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM prosecutors WHERE prosecutor_id = $1")
                .bind(pid)
                .fetch_optional(pool)
                .await?;
        if let Some(name) = name {
            if let Some(entry) = resolve_and_link(
                pool,
                ledger,
                case_id,
                &header.jurisdiction,
                "prosecutor",
                name,
                Some(office),
                Some(pid),
            )
            .await?
            {
                out.linked.push(entry);
            }
        }
    }

    if let Some(judge) = header
        .judge
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        // A judge field may name a whole panel. Each member is an individual
        // and is linked as one; a field that cannot be read as individuals is
        // queued for a human instead of being guessed at.
        match vi_reckoning::parse_officials(judge) {
            vi_reckoning::Officials::Individuals(names) => {
                for name in names {
                    if let Some(entry) = resolve_and_link(
                        pool,
                        ledger,
                        case_id,
                        &header.jurisdiction,
                        "judge",
                        name,
                        None,
                        None,
                    )
                    .await?
                    {
                        out.linked.push(entry);
                    }
                }
            }
            vi_reckoning::Officials::Ambiguous { reason } => {
                record_unresolved(pool, case_id, "judge", judge, "ambiguous", &reason).await?;
                out.unresolved.push(json!({
                    "role": "judge",
                    "raw_value": judge,
                    "kind": "ambiguous",
                    "reason": reason,
                }));
            }
            vi_reckoning::Officials::Collective { reason } => {
                record_unresolved(pool, case_id, "judge", judge, "collective", &reason).await?;
                out.unresolved.push(json!({
                    "role": "judge",
                    "raw_value": judge,
                    "kind": "collective",
                    "reason": reason,
                }));
            }
        }
    }

    Ok(out)
}

/// Judge and counsel fields that named no readable individual.
///
/// An ambiguous entry is a question for a human: which individuals does this
/// field name? Until someone answers, nobody is named and nobody accrues a
/// record from it.
pub async fn unresolved_officials(pool: &PgPool, include_resolved: bool) -> Result<Value, Error> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(x ORDER BY x->>'first_seen_at' DESC), '[]'::jsonb) FROM (
            SELECT jsonb_build_object(
                     'unresolved_id', u.unresolved_id,
                     'case_id', u.case_id,
                     'docket_number', c.docket_number,
                     'jurisdiction', c.jurisdiction,
                     'role_in_case', u.role_in_case,
                     'raw_value', u.raw_value,
                     'reason_kind', u.reason_kind,
                     'reason', u.reason,
                     'source_url', c.source_url,
                     'resolved_at', u.resolved_at,
                     'resolution', u.resolution,
                     'first_seen_at', u.first_seen_at) AS x
              FROM unresolved_officials u
              JOIN court_cases c ON c.case_id = u.case_id
             WHERE $1 OR u.resolved_at IS NULL
             ORDER BY u.first_seen_at DESC
             LIMIT 500) t",
    )
    .bind(include_resolved)
    .fetch_one(pool)
    .await?;

    Ok(json!({
        "entries": rows,
        "note": "Nothing listed here is attributed to any individual. An ambiguous \
                 field names several officials and the text does not say where one \
                 name ends; only a human can close it.",
    }))
}

/// Park a judge or counsel field that names no readable individual.
///
/// Nothing here is attributed to anyone. An ambiguous entry is a question for a
/// human; a collective one records that the court acted as a body.
async fn record_unresolved(
    pool: &PgPool,
    case_id: Uuid,
    role_in_case: &str,
    raw_value: &str,
    reason_kind: &str,
    reason: &str,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO unresolved_officials
           (case_id, role_in_case, raw_value, reason_kind, reason)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (case_id, role_in_case, raw_value) DO NOTHING",
    )
    .bind(case_id)
    .bind(role_in_case)
    .bind(raw_value)
    .bind(reason_kind)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn resolve_and_link(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
    jurisdiction: &str,
    role: &'static str,
    name: String,
    office: Option<String>,
    prosecutor_id: Option<Uuid>,
) -> Result<Option<Value>, Error> {
    let hit = vi_reckoning::resolve(
        pool,
        ledger,
        &vi_reckoning::ResolveQuery {
            role: role.to_string(),
            name,
            jurisdiction: jurisdiction.to_string(),
            office,
            bar_number: None,
            badge_number: None,
            prosecutor_id,
        },
    )
    .await;

    let hit = match hit {
        Ok(hit) => hit,
        // A record with no usable name is not an identity; nothing to link.
        Err(vi_reckoning::Error::InvalidName) => return Ok(None),
        Err(e) => {
            tracing::warn!(role, error = %e, "could not resolve an individual");
            return Ok(None);
        }
    };

    sqlx::query(
        "INSERT INTO actor_case_links (actor_id, case_id, role_in_case)
         VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
    )
    .bind(hit.actor.actor_id)
    .bind(case_id)
    .bind(role)
    .execute(pool)
    .await?;

    // Flags on this case concern this individual. The link gives review a
    // subject; it publishes nothing and asserts nothing.
    sqlx::query(
        "UPDATE abuse_flags SET actor_id = $1
          WHERE case_id = $2 AND actor_id IS NULL
            AND ($3::uuid IS NULL OR prosecutor_id IS NULL OR prosecutor_id = $3)",
    )
    .bind(hit.actor.actor_id)
    .bind(case_id)
    .bind(prosecutor_id)
    .execute(pool)
    .await?;

    Ok(Some(json!({
        "actor_id": hit.actor.actor_id,
        "role": role,
        "display_name": hit.actor.display_name,
        "method": hit.method,
        "confidence": hit.confidence,
    })))
}

/// What the pipeline has done so far, and what is still waiting.
pub async fn status(pool: &PgPool) -> Result<Value, Error> {
    let (total, pending): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM court_cases),
                (SELECT COUNT(*) FROM court_cases c
                  WHERE NOT EXISTS (SELECT 1 FROM pipeline_runs r
                                     WHERE r.case_id = c.case_id
                                       AND r.status IN ('ok','partial')))",
    )
    .fetch_one(pool)
    .await?;

    let by_status = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_object_agg(status, n), '{}'::jsonb)
           FROM (SELECT status, COUNT(*) AS n FROM pipeline_runs GROUP BY status) s",
    )
    .fetch_one(pool)
    .await?;

    let totals = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
                  'runs', COUNT(*),
                  'screens', COUNT(screen_id),
                  'flags_fired', COALESCE(SUM(flags_fired), 0),
                  'evidence_gaps', COALESCE(SUM(evidence_gaps), 0),
                  'actors_linked', COALESCE(SUM(actors_linked), 0))
           FROM pipeline_runs",
    )
    .fetch_one(pool)
    .await?;

    let recent = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(x ORDER BY x->>'started_at' DESC), '[]'::jsonb) FROM (
            SELECT jsonb_build_object(
                     'run_id', r.run_id,
                     'case_id', r.case_id,
                     'docket_number', c.docket_number,
                     'jurisdiction', c.jurisdiction,
                     'status', r.status,
                     'trigger', r.trigger,
                     'screen_hits', r.screen_hits,
                     'flags_fired', r.flags_fired,
                     'evidence_gaps', r.evidence_gaps,
                     'actors_linked', r.actors_linked,
                     'started_at', r.started_at) AS x
              FROM pipeline_runs r
              JOIN court_cases c ON c.case_id = r.case_id
             ORDER BY r.started_at DESC LIMIT 25) t",
    )
    .fetch_one(pool)
    .await?;

    let unresolved = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
                  'open_ambiguous', COUNT(*) FILTER (
                      WHERE reason_kind = 'ambiguous' AND resolved_at IS NULL),
                  'open_collective', COUNT(*) FILTER (
                      WHERE reason_kind = 'collective' AND resolved_at IS NULL),
                  'closed', COUNT(*) FILTER (WHERE resolved_at IS NOT NULL))
           FROM unresolved_officials",
    )
    .fetch_one(pool)
    .await?;

    let latest_report = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
                  'report_id', report_id,
                  'trigger', trigger,
                  'cases_processed', cases_processed,
                  'stage_counts', stage_counts,
                  'created_at', created_at)
           FROM pipeline_reports ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(json!({
        "cases": { "total": total, "awaiting_pipeline": pending },
        "runs_by_status": by_status,
        "unresolved_officials": unresolved,
        "totals": totals,
        "recent": recent,
        "stages": STAGES,
        "latest_report": latest_report,
        "note": "Everything the pipeline produces is pending review. Publication is a \
                 separate act by licensed counsel.",
    }))
}
