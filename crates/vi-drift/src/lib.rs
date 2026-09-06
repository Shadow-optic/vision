//! Doctrinal Drift Engine — Bayesian online changepoint detection over
//! per-(court, clause) outcome-signal series.
//!
//! Pipeline: [`db::ingest_signals`] builds machine-derived outcome proxies
//! from ingested opinions (via the transparent [`lexicon`]) wherever a
//! constitution screen linked the opinion's case to a clause;
//! [`db::detect`] runs the pure-Rust [`bocpd`] core (Adams-MacKay) over one
//! series; [`db::list_changepoints`] surfaces recorded candidates.
//!
//! Every artifact is `pending` and advisory: a changepoint is a lead about
//! shifting outcomes for counsel to review, never a finding.
#![forbid(unsafe_code)]

pub mod bocpd;
pub mod db;
pub mod lexicon;

pub use db::{
    detect, ingest_signals, list_changepoints, Changepoint, Error, IngestReport,
    DETECTION_THRESHOLD, MIN_OBSERVATIONS,
};
