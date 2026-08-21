//! Persist corpus, holdings, 50-state analogs, and screen runs.
#![forbid(unsafe_code)]

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

use crate::corpus::{self, PROVISIONS};
use crate::holdings::{self, HOLDINGS, SPLITS};
use crate::jurisdictions::{self, JURISDICTIONS, SNAPSHOT_ID};
use crate::screen::{self, ScreenInput, ScreenReport};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("case not found")]
    NotFound,
    #[error("unknown jurisdiction: {0}")]
    UnknownJurisdiction(String),
    #[error("render: {0}")]
    Render(#[from] crate::report::RenderError),
}

pub async fn sync_native(pool: &PgPool) -> Result<(), Error> {
    for j in JURISDICTIONS {
        sqlx::query(
            "INSERT INTO jurisdiction_circuits
                (code, name, kind, circuit, selectable, sort_order)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (code) DO UPDATE SET
                name = EXCLUDED.name,
                kind = EXCLUDED.kind,
                circuit = EXCLUDED.circuit,
                selectable = EXCLUDED.selectable,
                sort_order = EXCLUDED.sort_order",
        )
        .bind(j.code)
        .bind(j.name)
        .bind(forum_kind_sql(j.kind))
        .bind(j.circuit)
        .bind(j.selectable)
        .bind(j.sort_order as i32)
        .execute(pool)
        .await?;
    }

    for p in PROVISIONS {
        sqlx::query(
            "INSERT INTO constitution_provisions
                (provision_id, kind, parent_id, citation_label, sort_order, body)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (provision_id) DO UPDATE SET
                kind = EXCLUDED.kind,
                parent_id = EXCLUDED.parent_id,
                citation_label = EXCLUDED.citation_label,
                sort_order = EXCLUDED.sort_order,
                body = EXCLUDED.body",
        )
        .bind(p.id)
        .bind(kind_sql(p.kind))
        .bind(p.parent_id)
        .bind(p.citation_label)
        .bind(p.sort_order as i32)
        .bind(p.body)
        .execute(pool)
        .await?;
    }

    for h in HOLDINGS {
        sqlx::query(
            "INSERT INTO constitution_holdings
                (holding_id, citation, year, court_kind, court_id, authority,
                 rule_statement, superseded_by, snapshot_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (holding_id) DO UPDATE SET
                citation = EXCLUDED.citation,
                year = EXCLUDED.year,
                court_kind = EXCLUDED.court_kind,
                court_id = EXCLUDED.court_id,
                authority = EXCLUDED.authority,
                rule_statement = EXCLUDED.rule_statement,
                superseded_by = EXCLUDED.superseded_by,
                snapshot_id = EXCLUDED.snapshot_id",
        )
        .bind(h.id)
        .bind(h.citation)
        .bind(h.year)
        .bind(court_kind_sql(h.court_kind))
        .bind(h.court_id)
        .bind(authority_sql(h.authority))
        .bind(h.rule_statement)
        .bind(h.superseded_by)
        .bind(SNAPSHOT_ID)
        .execute(pool)
        .await?;
        sqlx::query("DELETE FROM constitution_holding_clauses WHERE holding_id = $1")
            .bind(h.id)
            .execute(pool)
            .await?;
        for cid in h.clause_ids {
            sqlx::query(
                "INSERT INTO constitution_holding_clauses (holding_id, clause_id)
                 VALUES ($1,$2) ON CONFLICT DO NOTHING",
            )
            .bind(h.id)
            .bind(*cid)
            .execute(pool)
            .await?;
        }
    }

    for s in SPLITS {
        sqlx::query(
            "INSERT INTO constitution_splits
                (split_id, clause_id, question, side_a_circuits, side_a_view,
                 side_b_circuits, side_b_view, notes, snapshot_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (split_id) DO UPDATE SET
                clause_id = EXCLUDED.clause_id,
                question = EXCLUDED.question,
                side_a_circuits = EXCLUDED.side_a_circuits,
                side_a_view = EXCLUDED.side_a_view,
                side_b_circuits = EXCLUDED.side_b_circuits,
                side_b_view = EXCLUDED.side_b_view,
                notes = EXCLUDED.notes,
                snapshot_id = EXCLUDED.snapshot_id",
        )
        .bind(s.id)
        .bind(s.clause_id)
        .bind(s.question)
        .bind(
            s.side_a_circuits
                .iter()
                .map(|c| (*c).to_string())
                .collect::<Vec<_>>(),
        )
        .bind(s.side_a_view)
        .bind(
            s.side_b_circuits
                .iter()
                .map(|c| (*c).to_string())
                .collect::<Vec<_>>(),
        )
        .bind(s.side_b_view)
        .bind(s.notes)
        .bind(SNAPSHOT_ID)
        .execute(pool)
        .await?;
    }

    for a in holdings::all_state_analogs() {
        sqlx::query(
            "INSERT INTO constitution_state_analogs
                (code, clause_id, state_citation, relation, more_protective, notes)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (code, clause_id) DO UPDATE SET
                state_citation = EXCLUDED.state_citation,
                relation = EXCLUDED.relation,
                more_protective = EXCLUDED.more_protective,
                notes = EXCLUDED.notes",
        )
        .bind(a.code)
        .bind(a.clause_id)
        .bind(a.state_citation)
        .bind(relation_sql(a.relation))
        .bind(a.more_protective)
        .bind(a.notes)
        .execute(pool)
        .await?;
    }

    let hash = corpus::corpus_hash();
    sqlx::query(
        "INSERT INTO constitution_meta (key, value) VALUES ('corpus_hash', $1)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(&hash)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO constitution_meta (key, value) VALUES ('snapshot_id', $1)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(SNAPSHOT_ID)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO constitution_meta (key, value) VALUES ('corpus_id', $1)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(corpus::CORPUS_ID)
    .execute(pool)
    .await?;

    Ok(())
}

fn kind_sql(k: corpus::ProvisionKind) -> &'static str {
    match k {
        corpus::ProvisionKind::Preamble => "preamble",
        corpus::ProvisionKind::Article => "article",
        corpus::ProvisionKind::Section => "section",
        corpus::ProvisionKind::Amendment => "amendment",
    }
}

fn forum_kind_sql(k: crate::jurisdictions::ForumKind) -> &'static str {
    match k {
        crate::jurisdictions::ForumKind::State => "state",
        crate::jurisdictions::ForumKind::District => "district",
        crate::jurisdictions::ForumKind::Territory => "territory",
        crate::jurisdictions::ForumKind::Federal => "federal",
    }
}

fn court_kind_sql(k: crate::holdings::CourtKind) -> &'static str {
    match k {
        crate::holdings::CourtKind::Scotus => "scotus",
        crate::holdings::CourtKind::Circuit => "circuit",
        crate::holdings::CourtKind::State => "state",
    }
}

fn authority_sql(a: crate::holdings::Authority) -> &'static str {
    match a {
        crate::holdings::Authority::Controlling => "controlling",
        crate::holdings::Authority::CircuitBinding => "circuit_binding",
        crate::holdings::Authority::Persuasive => "persuasive",
        crate::holdings::Authority::Split => "split",
        crate::holdings::Authority::Overruled => "overruled",
    }
}

fn relation_sql(r: crate::holdings::Relation) -> &'static str {
    match r {
        crate::holdings::Relation::Independent => "independent",
        crate::holdings::Relation::Lockstep => "lockstep",
        crate::holdings::Relation::Unspecified => "unspecified",
    }
}

fn status_sql(s: crate::resolve::ResolveStatus) -> &'static str {
    match s {
        crate::resolve::ResolveStatus::Controlling => "controlling",
        crate::resolve::ResolveStatus::CircuitBinding => "circuit_binding",
        crate::resolve::ResolveStatus::Unsettled => "unsettled",
        crate::resolve::ResolveStatus::Inapplicable => "inapplicable",
        crate::resolve::ResolveStatus::NoHolding => "no_holding",
    }
}

fn severity_sql(s: crate::screen::Severity) -> &'static str {
    match s {
        crate::screen::Severity::Low => "low",
        crate::screen::Severity::Medium => "medium",
        crate::screen::Severity::High => "high",
        crate::screen::Severity::Critical => "critical",
    }
}

pub async fn search_provisions(
    pool: &PgPool,
    q: &str,
    limit: i64,
) -> Result<Vec<serde_json::Value>, Error> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT jsonb_build_object(
             'provision_id', provision_id,
             'kind', kind,
             'citation_label', citation_label,
             'body', left(body, 800),
             'rank', ts_rank(tsv, websearch_to_tsquery('english', $1))
           )
           FROM constitution_provisions
           WHERE tsv @@ websearch_to_tsquery('english', $1)
           ORDER BY ts_rank(tsv, websearch_to_tsquery('english', $1)) DESC
           LIMIT $2"#,
    )
    .bind(q)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(sqlx::FromRow)]
struct CaseRow {
    jurisdiction: String,
    court_level: Option<String>,
    plea_offered: Option<bool>,
    plea_accepted: Option<bool>,
    outcome: Option<String>,
    plea_offer_months: Option<i32>,
    sentence_months: Option<i32>,
    evidence_strength: Option<String>,
    defendant_race: Option<String>,
    charge_category: Option<String>,
}

pub async fn screen_case(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
) -> Result<(Uuid, ScreenReport, String), Error> {
    let row = sqlx::query_as::<_, CaseRow>(
        "SELECT jurisdiction, court_level, plea_offered, plea_accepted, outcome,
                plea_offer_months, sentence_months, evidence_strength,
                defendant_race, charge_category
         FROM court_cases WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?;
    let row = row.ok_or(Error::NotFound)?;
    let jurisdiction = row.jurisdiction;
    let court_level = row.court_level;
    let plea_offered = row.plea_offered;
    let plea_accepted = row.plea_accepted;
    let outcome = row.outcome;
    let plea_offer_months = row.plea_offer_months;
    let sentence_months = row.sentence_months;
    let evidence_strength = row.evidence_strength;
    let defendant_race = row.defendant_race;
    let charge_category = row.charge_category;

    if jurisdictions::lookup(&jurisdiction).is_none() {
        return Err(Error::UnknownJurisdiction(jurisdiction));
    }

    let opinion: Option<String> = sqlx::query_scalar(
        "SELECT string_agg(full_text, E'\\n') FROM court_opinions WHERE case_id = $1",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await?;

    let brady_gaps: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT gaps_found FROM brady_recon_runs
          WHERE case_id = $1 ORDER BY run_at DESC LIMIT 1), 0)",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await?;

    let input = ScreenInput {
        jurisdiction: jurisdiction.clone(),
        court_level,
        plea_offered,
        plea_accepted,
        outcome,
        plea_offer_months,
        sentence_months,
        evidence_strength,
        defendant_race,
        charge_category,
        opinion_text: opinion,
        has_brady_gaps: brady_gaps > 0,
    };
    let report = screen::screen(&input);
    let md = crate::report::render(&report)?;
    let payload = serde_json::to_value(&report).unwrap_or_else(|_| json!({}));

    let screen_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO constitution_screens
            (screen_id, case_id, jurisdiction, circuit, snapshot_id, corpus_hash,
             hit_count, report, review_status)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'pending')",
    )
    .bind(screen_id)
    .bind(case_id)
    .bind(&report.jurisdiction)
    .bind(&report.circuit)
    .bind(report.snapshot_id)
    .bind(&report.corpus_hash)
    .bind(report.hits.len() as i32)
    .bind(&payload)
    .execute(pool)
    .await?;

    for hit in &report.hits {
        let matched: Vec<String> = hit.matched.clone();
        sqlx::query(
            "INSERT INTO constitution_screen_hits
                (screen_id, clause_id, authority, citation, severity, matched, resolution)
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(screen_id)
        .bind(hit.clause_id)
        .bind(status_sql(hit.resolution.status))
        .bind(hit.resolution.binding.as_ref().map(|b| b.citation))
        .bind(severity_sql(hit.severity))
        .bind(&matched)
        .bind(serde_json::to_value(&hit.resolution).unwrap_or_else(|_| json!({})))
        .execute(pool)
        .await?;
    }

    ledger
        .append(
            vi_ledger::events::CONSTITUTION_SCREEN_RUN,
            &json!({
                "screen_id": screen_id,
                "case_id": case_id,
                "jurisdiction": report.jurisdiction,
                "hit_count": report.hits.len(),
                "corpus_hash": report.corpus_hash,
            }),
        )
        .await?;

    Ok((screen_id, report, md))
}

pub async fn latest_screen(
    pool: &PgPool,
    case_id: Uuid,
) -> Result<Option<(Uuid, serde_json::Value, String)>, Error> {
    let row: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT screen_id, report FROM constitution_screens
         WHERE case_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?;
    let Some((id, report_json)) = row else {
        return Ok(None);
    };
    let md = markdown_from_stored(&report_json)
        .unwrap_or_else(|| "# Constitutional Screen — Attorney Work Product\n".into());
    Ok(Some((id, report_json, md)))
}

fn markdown_from_stored(v: &serde_json::Value) -> Option<String> {
    let report: ScreenReportStored = serde_json::from_value(v.clone()).ok()?;
    Some(render_stored(&report))
}

#[derive(serde::Deserialize)]
struct ScreenReportStored {
    jurisdiction: String,
    circuit: Option<String>,
    snapshot_id: String,
    corpus_hash: String,
    authority: String,
    hits: Vec<StoredHit>,
}

#[derive(serde::Deserialize)]
struct StoredHit {
    clause_id: String,
    label: String,
    severity: String,
    matched: Vec<String>,
}

fn render_stored(r: &ScreenReportStored) -> String {
    let mut md = format!(
        "# Constitutional Screen — Attorney Work Product\n**Jurisdiction:** {}{}\n**Snapshot:** {}\n**Corpus hash (BLAKE3):** `{}`\n**Authority:** {}\n\nGenerated by VisionInjustice. Requires licensed-attorney review before any use. This is **not legal advice**.\n\n",
        r.jurisdiction,
        r.circuit.as_deref().map(|c| format!(" ({c})")).unwrap_or_default(),
        r.snapshot_id,
        r.corpus_hash,
        r.authority
    );
    if r.hits.is_empty() {
        md.push_str("_No clause-level leads from the public-record facts supplied._\n");
    }
    for h in &r.hits {
        md.push_str(&format!(
            "## {} — {}\n**Clause:** `{}`\n\n",
            h.label, h.severity, h.clause_id
        ));
        for m in &h.matched {
            md.push_str(&format!("- {m}\n"));
        }
        md.push('\n');
    }
    md
}

pub async fn counts(pool: &PgPool) -> Result<(i64, i64, i64), Error> {
    let provisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM constitution_provisions")
        .fetch_one(pool)
        .await?;
    let holdings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM constitution_holdings")
        .fetch_one(pool)
        .await?;
    let analogs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM constitution_state_analogs")
        .fetch_one(pool)
        .await?;
    Ok((provisions, holdings, analogs))
}
