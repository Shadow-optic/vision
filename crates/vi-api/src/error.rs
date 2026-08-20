use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    pub fn internal(e: impl std::fmt::Display) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
    pub fn bad_req(m: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, m.into())
    }
    pub fn not_found() -> Self {
        Self(StatusCode::NOT_FOUND, "not found".into())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::internal(e)
    }
}
impl From<vi_ledger::Error> for ApiError {
    fn from(e: vi_ledger::Error) -> Self {
        Self::internal(e)
    }
}
impl From<vi_db::Error> for ApiError {
    fn from(e: vi_db::Error) -> Self {
        Self::internal(e)
    }
}
impl From<vi_monell_atlas::Error> for ApiError {
    fn from(e: vi_monell_atlas::Error) -> Self {
        match e {
            vi_monell_atlas::Error::InvalidType(s) | vi_monell_atlas::Error::InvalidStatus(s) => {
                Self::bad_req(s)
            }
            vi_monell_atlas::Error::NotFound => Self::not_found(),
            _ => Self::internal(e),
        }
    }
}
impl From<vi_brady_recon::Error> for ApiError {
    fn from(e: vi_brady_recon::Error) -> Self {
        match e {
            vi_brady_recon::Error::InvalidType(s) => Self::bad_req(s),
            _ => Self::internal(e),
        }
    }
}
impl From<vi_trial_penalty::Error> for ApiError {
    fn from(e: vi_trial_penalty::Error) -> Self {
        Self::internal(e)
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::bad_req(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}
