//! Reckoning Engine — individual accountability from public records.
//!
//! Focus is named humans (prosecutor, officer, judge, expert), not offices.
//! Licensed counsel reviews every finding. Once a finding is substantiated
//! from public records, the official's public-record identity and those
//! findings are published on the Wall of Injustice. The engine does not
//! file charges — humans do that. After a conviction, packages advocate
//! for the statutory maximum the same law provides, including life
//! imprisonment where 18 U.S.C. §§ 241, 242, or 1512 authorize it.
//! Pending automated flags never publish.
#![forbid(unsafe_code)]

pub mod dashboard;
pub mod entity;
pub mod package;
pub mod report;
pub mod score;
pub mod sentencing;
pub mod statutes;

use thiserror::Error;

/// When does a finding or flag concern a particular individual?
///
/// Either counsel attached it to that individual directly (`actor_id`), or it
/// names the prosecutor record that individual resolves to. Without the first
/// arm, only officials who happen to exist in the seeded `prosecutors` table
/// could ever accumulate a record — and a judge named in a live opinion could
/// abuse their office indefinitely without the engine being able to say so.
///
/// Two spellings of the same rule: one for queries that bind the individual
/// (`$1` actor id, `$2` prosecutor id), one for queries that join
/// `accountability_actors a`.
pub(crate) const FINDING_MATCHES_ACTOR_PARAMS: &str = "(f.actor_id = $1
       OR (f.actor_id IS NULL AND $2::uuid IS NOT NULL AND f.prosecutor_id = $2))";

pub(crate) const FINDING_MATCHES_ACTOR_ROW: &str = "(f.actor_id = a.actor_id
       OR (f.actor_id IS NULL AND a.prosecutor_id IS NOT NULL
           AND f.prosecutor_id = a.prosecutor_id))";

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
pub use entity::{
    list, parse_officials, renormalize, resolve, sync_from_public_records, Actor, Officials,
    ResolveHit, ResolveQuery,
};
pub use package::{generate, get_package, list_packages, StoredPackage};
pub use score::{score_actor, AbuseScore};
