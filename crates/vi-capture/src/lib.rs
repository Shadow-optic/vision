//! Structural Capture Graph — repeat-appearance concentration.
//!
//! [`db::rebuild_edges`] builds judge x court x outcome-signal edges from
//! ingested opinions (author from court_opinions.judge, outcome proxy from
//! the shared vi-drift lexicon). [`db::compute_metrics`] scores each entity's
//! outcome concentration (Gini + Shannon entropy) against a degree-preserving
//! Monte Carlo null ([`stats`], seeded rand_chacha, >= 1000 permutations).
//! [`db::outliers`] lists the strongest leads.
//!
//! All artifacts are machine-derived and `pending`: a low empirical p-value
//! means "worth counsel's review", never "captured".
#![forbid(unsafe_code)]

pub mod db;
pub mod stats;

pub use db::{
    compute_metrics, outliers, rebuild_edges, CaptureReport, Error, MetricOutlier,
    RebuildReport, FLAG_MAX_P, MIN_APPEARANCES, MIN_PERMUTATIONS,
};
