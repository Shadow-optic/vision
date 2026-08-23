//! Reckoning Engine — individual accountability from public records.
//!
//! Focus is named humans (prosecutor, officer, judge, expert), not offices.
//! Outputs are attorney work product: criminal-referral drafts, §1983
//! scaffolds, bar complaints, and statutory-range research. Nothing is
//! published against a named person until an Evidence Review Committee
//! substantiates findings and separately approves publication.
//!
//! The engine is not a charging authority, does not file documents, and
//! does not recommend a sentence.
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
    #[error("publication requires at least one substantiated finding")]
    PublicationBlocked,
}

pub use dashboard::{set_publication, tracker, wall, TrackerRow, WallEntry};
pub use entity::{list, resolve, sync_from_public_records, Actor, ResolveHit, ResolveQuery};
pub use package::{generate, get_package, list_packages, StoredPackage};
pub use score::{score_actor, AbuseScore};
