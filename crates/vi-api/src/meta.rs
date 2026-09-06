//! Routes for the transparency, resonance, drift, and capture engines.
//!
//! Resonance, drift, and capture artifacts are machine-derived and `pending`
//! by construction; these routes expose them to operators and counsel but they
//! are deliberately absent from the public proxy allowlist. Transparency
//! snapshots are audit artifacts over the *published* dataset only.
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiError;
use crate::handlers::AppState;

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect(),
    )
}

fn snapshot_json(s: &vi_transparency::Snapshot) -> Value {
    json!({
        "id": s.id,
        "created_at": s.created_at,
        "merkle_root": hex_encode(&s.merkle_root),
        "tree_size": s.tree_size,
        "table_counts": s.table_counts,
        "prev_root": s.prev_root.as_deref().map(hex_encode),
        "ledger_seq": s.ledger_seq,
    })
}

// ===== Transparency ========================================================

/// Capture a transparency snapshot of the published dataset and anchor its
/// Merkle root in the ledger. On-demand only — never per pipeline cycle.
pub async fn snapshot_trigger(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let snap = vi_transparency::snapshot(&st.pool).await?;
    Ok(Json(snapshot_json(&snap)))
}

pub async fn snapshot_list(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_as::<_, vi_transparency::Snapshot>(
        "SELECT id, created_at, merkle_root, tree_size, table_counts, prev_root, ledger_seq
           FROM transparency_snapshots ORDER BY created_at DESC, id DESC LIMIT 100",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        json!({ "snapshots": rows.iter().map(snapshot_json).collect::<Vec<_>>() }),
    ))
}

pub async fn snapshot_latest(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let snap = vi_transparency::latest_snapshot(&st.pool)
        .await?
        .ok_or_else(|| ApiError::not_found())?;
    Ok(Json(snapshot_json(&snap)))
}

/// Inclusion proof that a published row was part of a snapshot. 404 means the
/// row was never snapshotted (unpublished, or no snapshot taken yet).
pub async fn inclusion_proof(
    State(st): State<AppState>,
    Path((table, row_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let proof = vi_transparency::inclusion_proof(&st.pool, &table, &row_id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let snap = vi_transparency::latest_snapshot(&st.pool).await?;
    Ok(Json(json!({
        "proof": proof,
        "leaf_hash_hex": hex_encode(&proof.leaf_hash),
        // Root of the snapshot this proof was drawn from, for convenience.
        "snapshot_root": snap
            .filter(|s| s.id == proof.snapshot_id)
            .map(|s| hex_encode(&s.merkle_root)),
    })))
}

#[derive(Deserialize)]
pub struct VerifyBody {
    pub proof: vi_transparency::Proof,
    /// Hex-encoded 32-byte Merkle root to verify against.
    pub root: String,
}

/// Stateless verification of an inclusion proof against a caller-supplied
/// root. Returns `{ "valid": bool }`; a malformed root is `valid: false`,
/// never an error.
pub async fn verify_proof(Json(body): Json<VerifyBody>) -> Result<Json<Value>, ApiError> {
    let valid = match hex_decode(&body.root) {
        Some(root) => vi_transparency::verify_proof(&body.proof, &root),
        None => false,
    };
    Ok(Json(json!({ "valid": valid })))
}

// ===== Resonance ===========================================================

/// Recompute weak-signal fusion across the corpus. Pending, machine-derived
/// leads; one ledger event per run.
pub async fn resonance_compute(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let report = vi_resonance::compute_all(&st.pool).await?;
    Ok(Json(json!({
        "scored": report.scored,
        "surfaced": report.surfaced,
        "surface_q": vi_resonance::SURFACE_Q,
        "status": "pending",
        "note": "Machine-derived research leads. Nothing here is a finding or publishes anything.",
    })))
}

#[derive(Deserialize)]
pub struct ResonanceListQ {
    pub max_q: Option<f64>,
}

pub async fn resonance_cases(
    State(st): State<AppState>,
    Query(q): Query<ResonanceListQ>,
) -> Result<Json<Value>, ApiError> {
    let max_q = q.max_q.unwrap_or(vi_resonance::SURFACE_Q);
    let rows = sqlx::query_as::<_, vi_resonance::CaseResonance>(
        "SELECT case_id, fisher_chi2, fisher_p, stouffer_z, q_value, n_signals,
                signals, computed_at, status
           FROM case_resonance WHERE q_value <= $1 ORDER BY q_value ASC, case_id",
    )
    .bind(max_q)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({
        "max_q": max_q,
        "cases": rows,
        "note": "Pending machine-derived scores, sorted by Benjamini-Hochberg q-value.",
    })))
}

pub async fn resonance_case(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let row = vi_resonance::case_detail(&st.pool, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(json!(row)))
}

// ===== Drift ===============================================================

/// Default expected segment length for the constant-hazard BOCPD model.
pub const DRIFT_DEFAULT_HAZARD: f64 = 50.0;

/// Rebuild per-(court, clause) outcome-signal observations from ingested
/// opinions. Idempotent; appends one ledger event.
pub async fn drift_ingest(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let report = vi_drift::ingest_signals(&st.pool).await?;
    Ok(Json(json!(report)))
}

#[derive(Deserialize)]
pub struct DriftDetectBody {
    pub hazard: Option<f64>,
}

/// Run changepoint detection over one (court, clause) series. A series with
/// too few observations records a run with zero changepoints — the gap is
/// visible rather than silent.
pub async fn drift_detect(
    State(st): State<AppState>,
    Path((court_id, clause_id)): Path<(String, String)>,
    body: Option<Json<DriftDetectBody>>,
) -> Result<Json<Value>, ApiError> {
    let hazard = body.and_then(|b| b.hazard).unwrap_or(DRIFT_DEFAULT_HAZARD);
    let cps = vi_drift::detect(&st.pool, &court_id, &clause_id, hazard).await?;
    Ok(Json(json!({
        "court_id": court_id,
        "clause_id": clause_id,
        "hazard": hazard,
        "changepoints": cps,
        "status": "pending",
    })))
}

#[derive(Deserialize)]
pub struct ChangepointsQ {
    pub min_posterior: Option<f64>,
}

pub async fn drift_changepoints(
    State(st): State<AppState>,
    Query(q): Query<ChangepointsQ>,
) -> Result<Json<Value>, ApiError> {
    let min = q.min_posterior.unwrap_or(vi_drift::DETECTION_THRESHOLD);
    let rows = vi_drift::list_changepoints(&st.pool, min).await?;
    Ok(Json(json!({ "min_posterior": min, "changepoints": rows })))
}

// ===== Capture =============================================================

/// Rebuild the judge x court x outcome-signal edge table from ingested
/// opinions. Wholesale, idempotent, ledger-chained.
pub async fn capture_rebuild(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let report = vi_capture::rebuild_edges(&st.pool).await?;
    Ok(Json(json!(report)))
}

#[derive(Deserialize)]
pub struct CaptureComputeBody {
    pub permutations: Option<u32>,
    pub seed: Option<u64>,
}

/// Compute concentration metrics against a seeded Monte Carlo null. Both
/// parameters are echoed in the ledger event so a run is reproducible.
pub async fn capture_compute(
    State(st): State<AppState>,
    body: Option<Json<CaptureComputeBody>>,
) -> Result<Json<Value>, ApiError> {
    let (permutations, seed) = match body {
        Some(Json(b)) => (b.permutations, b.seed),
        None => (None, None),
    };
    let permutations = permutations.unwrap_or(vi_capture::MIN_PERMUTATIONS);
    let seed = seed.unwrap_or(0);
    let report = vi_capture::compute_metrics(&st.pool, permutations, seed).await?;
    Ok(Json(json!(report)))
}

#[derive(Deserialize)]
pub struct OutliersQ {
    pub max_p: Option<f64>,
}

pub async fn capture_outliers(
    State(st): State<AppState>,
    Query(q): Query<OutliersQ>,
) -> Result<Json<Value>, ApiError> {
    let max_p = q.max_p.unwrap_or(vi_capture::FLAG_MAX_P);
    let rows = vi_capture::outliers(&st.pool, max_p).await?;
    Ok(Json(json!({
        "max_p": max_p,
        "outliers": rows,
        "note": "Pending machine-derived concentration leads; a low p-value means \
                 'worth counsel's review', never 'captured'.",
    })))
}
