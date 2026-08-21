//! Native U.S. Constitution / Bill of Rights engine: corpus, stare decisis
//! resolver, 50-state analogs, and advisory case screening.
#![forbid(unsafe_code)]

pub mod clauses;
pub mod corpus;
pub mod db;
pub mod holdings;
pub mod jurisdictions;
pub mod report;
pub mod resolve;
pub mod screen;

use serde::Serialize;

pub use clauses::CLAUSES;
pub use corpus::{corpus_hash, CORPUS_ID, FROZEN_CORPUS_HASH, PROVISIONS};
pub use jurisdictions::{JURISDICTIONS, SNAPSHOT_ID};
pub use resolve::{resolve, Resolution, ResolveQuery};
pub use screen::{attach_features, screen, ScreenInput, ScreenReport};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Resolve(#[from] resolve::ResolveError),
    #[error(transparent)]
    Db(#[from] db::Error),
    #[error(transparent)]
    Render(#[from] report::RenderError),
}

#[derive(Debug, Serialize)]
pub struct Dropdowns {
    pub jurisdictions: Vec<jurisdictions::Jurisdiction>,
    pub circuits: &'static [jurisdictions::Circuit],
    pub court_levels: &'static [jurisdictions::CourtLevelOpt],
    pub clauses: &'static [clauses::Clause],
}

pub fn dropdowns() -> Dropdowns {
    Dropdowns {
        jurisdictions: JURISDICTIONS
            .iter()
            .copied()
            .filter(|j| j.selectable)
            .collect(),
        circuits: jurisdictions::CIRCUITS,
        court_levels: jurisdictions::COURT_LEVELS,
        clauses: CLAUSES,
    }
}

pub fn catalog() -> serde_json::Value {
    serde_json::json!({
        "name": "Constitution / Bill of Rights",
        "crate": "vi-constitution",
        "corpus_id": CORPUS_ID,
        "corpus_hash": corpus_hash(),
        "snapshot_id": SNAPSHOT_ID,
        "provisions": PROVISIONS.len(),
        "amendments": corpus::amendments().count(),
        "bill_of_rights": corpus::bill_of_rights_amendments().count(),
        "clauses": CLAUSES.len(),
        "holdings": holdings::HOLDINGS.len(),
        "splits": holdings::SPLITS.len(),
        "states": jurisdictions::states().count(),
        "selectable_forums": JURISDICTIONS.len(),
        "disclaimer": "Advisory attorney-review leads only. Not legal advice. Holdings snapshot is curated and incomplete.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropdowns_cover_fifty_states() {
        let d = dropdowns();
        let states = d
            .jurisdictions
            .iter()
            .filter(|j| matches!(j.kind, jurisdictions::ForumKind::State))
            .count();
        assert_eq!(states, 50);
        assert_eq!(d.circuits.len(), 13);
        assert!(d.court_levels.iter().any(|c| c.id == "superior"));
        assert!(d.clauses.iter().any(|c| c.id == "amend.06.counsel"));
    }
}
