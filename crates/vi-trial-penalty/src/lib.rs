//! Trial Penalty Observatory — plea-rejected / trial-convicted sentence premiums.
//! Public-record statistics only. Motion templates are drafts for counsel review.
#![forbid(unsafe_code)]

pub mod disparity;
pub mod distribution;
pub mod motion;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
}
