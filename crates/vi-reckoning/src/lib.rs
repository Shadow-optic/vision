//! Reckoning Engine — individual accountability from public records.
//!
//! Focus is named humans (prosecutor, officer, judge, expert), not offices.
//! Licensed counsel reviews every finding. Once a finding is substantiated
//! from public records, the official's public-record identity and those
//! findings are published on the Wall of Injustice. The engine does not
//! charge, file, or sentence anyone — humans do that. Pending automated
//! flags never publish.
#![forbid(unsafe_code)]

pub mod dashboard;
pub mod entity;
pub mod package;
pub mod report;
pub mod score;
pub mod statutes;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("render: {0}")]
    Render(#[from] report::RenderError),
    #[error("invalid role: {0}")]
    InvalidRole(String),
    #[error("invalid action kind: {0}")]
    InvalidKind(String),
    #[error("name is empty after normalization")]
    InvalidName,
    #[error("actor not found")]
    NotFound,
    #[error("no substantiated public-record evidence for this actor")]
    InsufficientEvidence,
    #[error("cannot publish: no counsel-substantiated public-record finding")]
    PublicationBlocked,
}

pub use dashboard::{set_publication, tracker, wall, wall_profile, TrackerRow, WallEntry};
pub use entity::{list, resolve, sync_from_public_records, Actor, ResolveHit, ResolveQuery};
pub use package::{generate, get_package, list_packages, StoredPackage};
pub use score::{score_actor, AbuseScore};
