//! What happens to a record after it is ingested.
//!
//! Ingestion stores public records. This crate is the wiring that walks each
//! new record through every engine that has something to say about it:
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
//!
//! Every artifact this pipeline creates is pending by construction. Screens,
//! leads, flags, links, and scores publish nothing and accuse no one: the
//! Abuse Score counts only counsel-substantiated material, so a case that has
//! just been ingested moves nobody's score off zero. Publication remains a
//! separate, human, licensed-counsel act.
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
    #[error("case not found")]
    NotFound,
}

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
                Err(e) => stages.push(StageOutcome::failed(
                    "constitution_screen",
                    &e.to_string(),
                )),
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
                    Err(e) => {
                        stages.push(StageOutcome::failed("evidence_leads", &e.to_string()))
                    }
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
        Ok(linked) => {
            actors_linked = linked.len() as i32;
            stages.push(StageOutcome::ok(
                "actor_links",
                json!({ "linked": linked }),
            ));
        }
        Err(e) => stages.push(StageOutcome::failed("actor_links", &e.to_string())),
    }

    // --- 6. Score ---------------------------------------------------------
    let linked_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT actor_id FROM actor_case_links WHERE case_id = $1",
    )
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

    let rules =
        sqlx::query_as::<_, (Uuid, String)>("SELECT rule_id, source FROM abuse_rules WHERE enabled")
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
async fn link_actors(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
    header: &CaseHeader,
) -> Result<Vec<Value>, Error> {
    let mut linked = Vec::new();

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
                linked.push(entry);
            }
        }
    }

    if let Some(judge) = header
        .judge
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(entry) = resolve_and_link(
            pool,
            ledger,
            case_id,
            &header.jurisdiction,
            "judge",
            judge.to_string(),
            None,
            None,
        )
        .await?
        {
            linked.push(entry);
        }
    }

    Ok(linked)
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

    Ok(json!({
        "cases": { "total": total, "awaiting_pipeline": pending },
        "runs_by_status": by_status,
        "totals": totals,
        "recent": recent,
        "stages": [
            "forum", "constitution_screen", "evidence_leads",
            "abuse_rules", "actor_links", "score",
        ],
        "note": "Everything the pipeline produces is pending review. Publication is a \
                 separate act by licensed counsel.",
    }))
}
