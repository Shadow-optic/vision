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
impl From<vi_tactics::Error> for ApiError {
    fn from(e: vi_tactics::Error) -> Self {
        match e {
            vi_tactics::Error::InvalidCategory(s) | vi_tactics::Error::InvalidSignal(s) => {
                Self::bad_req(s)
            }
            vi_tactics::Error::NotFound => Self::not_found(),
            _ => Self::internal(e),
        }
    }
}
impl From<vi_geo::GeoError> for ApiError {
    fn from(e: vi_geo::GeoError) -> Self {
        Self::bad_req(e.to_string())
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        let msg = e.to_string();
        if msg.starts_with("unknown ingest source") {
            Self::bad_req(msg)
        } else {
            Self::internal(msg)
        }
    }
}
impl From<vi_constitution::Error> for ApiError {
    fn from(e: vi_constitution::Error) -> Self {
        match e {
            vi_constitution::Error::Resolve(r) => Self::bad_req(r.to_string()),
            vi_constitution::Error::Db(d) => d.into(),
            vi_constitution::Error::Render(r) => Self::internal(r),
        }
    }
}
impl From<vi_constitution::resolve::ResolveError> for ApiError {
    fn from(e: vi_constitution::resolve::ResolveError) -> Self {
        Self::bad_req(e.to_string())
    }
}
impl From<vi_constitution::db::Error> for ApiError {
    fn from(e: vi_constitution::db::Error) -> Self {
        match e {
            vi_constitution::db::Error::NotFound => Self::not_found(),
            vi_constitution::db::Error::UnknownJurisdiction(s) => Self::bad_req(s),
            vi_constitution::db::Error::Render(r) => Self::internal(r),
            other => Self::internal(other),
        }
    }
}
impl From<vi_reckoning::Error> for ApiError {
    fn from(e: vi_reckoning::Error) -> Self {
        match e {
            vi_reckoning::Error::InvalidRole(s) | vi_reckoning::Error::InvalidKind(s) => {
                Self::bad_req(s)
            }
            vi_reckoning::Error::InvalidName
            | vi_reckoning::Error::InsufficientEvidence
            | vi_reckoning::Error::PublicationBlocked => Self::bad_req(e.to_string()),
            vi_reckoning::Error::NotFound => Self::not_found(),
            other => Self::internal(other),
        }
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
