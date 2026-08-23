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
use vi_reckoning::entity::ResolveQuery;
use vi_reckoning::statutes;

#[derive(Deserialize)]
pub struct ActorListQ {
    pub role: Option<String>,
    pub jurisdiction: Option<String>,
}

pub async fn list_actors(
    State(st): State<AppState>,
    Query(q): Query<ActorListQ>,
) -> Result<Json<Value>, ApiError> {
    let actors = vi_reckoning::list(&st.pool, q.role.as_deref(), q.jurisdiction.as_deref()).await?;
    Ok(Json(json!({ "actors": actors })))
}

pub async fn get_actor(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let actor = vi_reckoning::entity::get(&st.pool, id).await?;
    let score = vi_reckoning::score_actor(&st.pool, None, id).await?;
    Ok(Json(json!({ "actor": actor, "score": score })))
}

pub async fn resolve(
    State(st): State<AppState>,
    Json(body): Json<ResolveQuery>,
) -> Result<Json<Value>, ApiError> {
    let hit = vi_reckoning::resolve(&st.pool, &st.ledger, &body).await?;
    Ok(Json(json!(hit)))
}

pub async fn sync(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let created = vi_reckoning::sync_from_public_records(&st.pool, &st.ledger).await?;
    Ok(Json(json!({ "created": created })))
}

pub async fn get_score(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        vi_reckoning::score_actor(&st.pool, None, id).await?
    )))
}

pub async fn persist_score(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        vi_reckoning::score_actor(&st.pool, Some(&st.ledger), id).await?
    )))
}

#[derive(Deserialize)]
pub struct PackageBody {
    pub kind: String,
}

pub async fn generate_package(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PackageBody>,
) -> Result<Json<Value>, ApiError> {
    let (pkg, md) = vi_reckoning::generate(&st.pool, &st.ledger, id, &body.kind).await?;
    Ok(Json(json!({
        "package_id": pkg.package_id,
        "actor_id": pkg.actor_id,
        "action_kind": pkg.action_kind,
        "status": pkg.status,
        "document_hash": pkg.document_hash,
        "markdown": md,
    })))
}

pub async fn get_package(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let pkg = vi_reckoning::get_package(&st.pool, id).await?;
    Ok((
        StatusCode::OK,
        [("content-type", "text/markdown")],
        pkg.body_markdown,
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct PackageListQ {
    pub actor_id: Option<Uuid>,
    pub kind: Option<String>,
}

pub async fn list_packages(
    State(st): State<AppState>,
    Query(q): Query<PackageListQ>,
) -> Result<Json<Value>, ApiError> {
    let rows = vi_reckoning::list_packages(&st.pool, q.actor_id, q.kind.as_deref()).await?;
    Ok(Json(json!({
        "packages": rows.iter().map(|p| json!({
            "package_id": p.package_id,
            "actor_id": p.actor_id,
            "action_kind": p.action_kind,
            "status": p.status,
            "document_hash": p.document_hash,
        })).collect::<Vec<_>>()
    })))
}

pub async fn wall(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let entries = vi_reckoning::wall(&st.pool).await?;
    Ok(Json(json!({
        "name": "Public Accountability Register",
        "also_known_as": "Wall of Injustice",
        "gate": "substantiated findings + Evidence Review Committee publication approval",
        "entries": entries,
    })))
}

pub async fn tracker(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        json!({ "packages": vi_reckoning::tracker(&st.pool).await? }),
    ))
}

#[derive(Deserialize)]
pub struct PublishBody {
    pub approved: bool,
    pub reviewer_id: Option<Uuid>,
    pub notes: Option<String>,
}

pub async fn publish(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PublishBody>,
) -> Result<Json<Value>, ApiError> {
    vi_reckoning::set_publication(
        &st.pool,
        &st.ledger,
        id,
        body.approved,
        body.reviewer_id,
        body.notes,
    )
    .await?;
    Ok(Json(json!({ "actor_id": id, "approved": body.approved })))
}

pub async fn statute_catalog() -> Json<Value> {
    Json(json!({
        "statutes": statutes::STATUTES,
        "note": "Research catalog. Not charging decisions. Willfulness and agreement are for counsel and a fact-finder.",
    }))
}

pub async fn immunity_catalog() -> Json<Value> {
    Json(json!({
        "doctrines": statutes::IMMUNITY,
        "note": "Immunity research is not a recommendation to ignore a shield or to proceed extra-legally.",
    }))
}
