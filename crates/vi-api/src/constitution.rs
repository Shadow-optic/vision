use crate::error::ApiError;
use crate::handlers::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use vi_constitution::corpus::{self, ProvisionKind};
use vi_constitution::resolve::ResolveQuery;

pub async fn catalog(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let (provisions, holdings, analogs) = vi_constitution::db::counts(&st.pool).await?;
    let mut cat = vi_constitution::catalog();
    if let Some(obj) = cat.as_object_mut() {
        obj.insert("provision_rows".into(), json!(provisions));
        obj.insert("holding_rows".into(), json!(holdings));
        obj.insert("state_analog_rows".into(), json!(analogs));
    }
    Ok(Json(cat))
}

pub async fn options() -> Json<Value> {
    Json(json!(vi_constitution::dropdowns()))
}

#[derive(Deserialize)]
pub struct JurQ {
    pub kind: Option<String>,
}

pub async fn jurisdictions(Query(q): Query<JurQ>) -> Json<Value> {
    let drop = vi_constitution::dropdowns();
    let items: Vec<_> = drop
        .jurisdictions
        .into_iter()
        .filter(|j| match q.kind.as_deref() {
            Some(k) => format!("{:?}", j.kind).eq_ignore_ascii_case(k),
            None => true,
        })
        .collect();
    Json(json!({
        "jurisdictions": items,
        "circuits": drop.circuits,
        "court_levels": drop.court_levels,
    }))
}

#[derive(Deserialize)]
pub struct ProvQ {
    pub kind: Option<String>,
}

pub async fn list_provisions(Query(q): Query<ProvQ>) -> Json<Value> {
    let kinds = q.kind.as_deref();
    let rows: Vec<_> = corpus::PROVISIONS
        .iter()
        .filter(|p| match kinds {
            Some("amendment") => p.kind == ProvisionKind::Amendment,
            Some("article") => p.kind == ProvisionKind::Article,
            Some("section") => p.kind == ProvisionKind::Section,
            Some("preamble") => p.kind == ProvisionKind::Preamble,
            Some("bill_of_rights") => matches!(
                p.id,
                "amend.01"
                    | "amend.02"
                    | "amend.03"
                    | "amend.04"
                    | "amend.05"
                    | "amend.06"
                    | "amend.07"
                    | "amend.08"
                    | "amend.09"
                    | "amend.10"
            ),
            _ => true,
        })
        .map(|p| {
            json!({
                "id": p.id,
                "kind": p.kind,
                "parent_id": p.parent_id,
                "citation_label": p.citation_label,
                "body": p.body,
            })
        })
        .collect();
    Json(json!({ "provisions": rows }))
}

pub async fn get_provision(Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let p = corpus::get(&id).ok_or_else(ApiError::not_found)?;
    Ok(Json(json!({
        "id": p.id,
        "kind": p.kind,
        "parent_id": p.parent_id,
        "citation_label": p.citation_label,
        "body": p.body,
    })))
}

pub async fn list_clauses() -> Json<Value> {
    Json(json!({ "clauses": vi_constitution::CLAUSES }))
}

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
    let hits = vi_constitution::db::search_provisions(&st.pool, &q.q, limit).await?;
    Ok(Json(json!({ "hits": hits })))
}

#[derive(Deserialize)]
pub struct ResolveBody {
    pub clause_id: String,
    pub jurisdiction: String,
    pub court_level: Option<String>,
    pub as_of_year: Option<i32>,
}

pub async fn resolve(Json(body): Json<ResolveBody>) -> Result<Json<Value>, ApiError> {
    let r = vi_constitution::resolve(&ResolveQuery {
        clause_id: body.clause_id,
        jurisdiction: body.jurisdiction,
        court_level: body.court_level,
        as_of_year: body.as_of_year,
    })?;
    Ok(Json(json!(r)))
}

pub async fn screen_run(
    State(st): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let (screen_id, report, _md) =
        vi_constitution::db::screen_case(&st.pool, &st.ledger, case_id).await?;
    Ok(Json(json!({
        "screen_id": screen_id,
        "case_id": case_id,
        "jurisdiction": report.jurisdiction,
        "circuit": report.circuit,
        "hit_count": report.hits.len(),
        "authority": report.authority,
        "review_status": "pending",
        "hits": report.hits,
        "disclaimer": "Advisory attorney-review leads only. Not legal advice."
    })))
}

pub async fn screen_report(
    State(st): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    match vi_constitution::db::latest_screen(&st.pool, case_id).await? {
        Some((_id, _json, md)) => {
            Ok((StatusCode::OK, [("content-type", "text/markdown")], md).into_response())
        }
        None => Err(ApiError::not_found()),
    }
}

#[derive(Deserialize)]
pub struct ScreenReviewBody {
    /// `substantiate` or `reject`.
    pub action: String,
    pub notes: Option<String>,
}

/// Counsel review of a constitution screen — the same gate the atlas applies
/// to findings. A screen is a machine-generated lead until this runs.
pub async fn screen_review(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ScreenReviewBody>,
) -> Result<Json<Value>, ApiError> {
    let status = match body.action.as_str() {
        "substantiate" => "substantiated",
        "reject" => "rejected",
        other => {
            return Err(ApiError::bad_req(format!(
                "unknown action '{other}'; expected 'substantiate' or 'reject'"
            )))
        }
    };
    vi_constitution::db::review_screen(&st.pool, &st.ledger, id, status, body.notes.as_deref())
        .await?;
    Ok(Json(json!({ "screen_id": id, "review_status": status })))
}
