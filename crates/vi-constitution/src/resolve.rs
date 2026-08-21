//! Stare decisis resolver: SCOTUS floor, forum circuit, splits, state analogs.
#![forbid(unsafe_code)]

use serde::Serialize;

use crate::clauses::{self, Clause, Incorporation};
use crate::holdings::{self, Authority, CourtKind, Holding, Relation, StateAnalog};
use crate::jurisdictions::{self, ForumKind, Jurisdiction};

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("unknown clause: {0}")]
    UnknownClause(String),
    #[error("unknown jurisdiction: {0}")]
    UnknownJurisdiction(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveQuery {
    pub clause_id: String,
    pub jurisdiction: String,
    pub court_level: Option<String>,
    pub as_of_year: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveStatus {
    Controlling,
    CircuitBinding,
    Unsettled,
    Inapplicable,
    NoHolding,
}

#[derive(Debug, Clone, Serialize)]
pub struct HoldingView {
    pub id: &'static str,
    pub citation: &'static str,
    pub year: i32,
    pub court_kind: CourtKind,
    pub court_id: &'static str,
    pub authority: Authority,
    pub rule_statement: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitView {
    pub id: &'static str,
    pub question: &'static str,
    pub side_a_circuits: &'static [&'static str],
    pub side_a_view: &'static str,
    pub side_b_circuits: &'static [&'static str],
    pub side_b_view: &'static str,
    pub notes: &'static str,
    pub forum_side: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalogView {
    pub code: &'static str,
    pub clause_id: &'static str,
    pub state_citation: &'static str,
    pub relation: Relation,
    pub more_protective: Option<bool>,
    pub notes: &'static str,
    pub state_above_federal: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    pub clause_id: &'static str,
    pub clause_label: &'static str,
    pub jurisdiction: &'static str,
    pub jurisdiction_name: &'static str,
    pub circuit: &'static str,
    pub state_actor: bool,
    pub incorporated: bool,
    pub skipped_unincorporated: bool,
    pub status: ResolveStatus,
    pub binding: Option<HoldingView>,
    pub circuit_holding: Option<HoldingView>,
    pub overruled: Vec<HoldingView>,
    pub split: Option<SplitView>,
    pub state_analog: Option<AnalogView>,
    pub notes: Vec<String>,
}

impl From<&Holding> for HoldingView {
    fn from(h: &Holding) -> Self {
        Self {
            id: h.id,
            citation: h.citation,
            year: h.year,
            court_kind: h.court_kind,
            court_id: h.court_id,
            authority: h.authority,
            rule_statement: h.rule_statement,
        }
    }
}

pub fn resolve(q: &ResolveQuery) -> Result<Resolution, ResolveError> {
    let clause = clauses::get(&q.clause_id)
        .ok_or_else(|| ResolveError::UnknownClause(q.clause_id.clone()))?;
    let jur = jurisdictions::lookup(&q.jurisdiction)
        .ok_or_else(|| ResolveError::UnknownJurisdiction(q.jurisdiction.clone()))?;
    Ok(resolve_known(
        clause,
        jur,
        q.court_level.as_deref(),
        q.as_of_year,
    ))
}

fn year_ok(h: &Holding, as_of: Option<i32>) -> bool {
    as_of.map(|y| h.year <= y).unwrap_or(true)
}

fn is_live(h: &Holding, as_of: Option<i32>) -> bool {
    year_ok(h, as_of) && h.authority != Authority::Overruled && h.superseded_by.is_none()
}

fn resolve_known(
    clause: &Clause,
    jur: &Jurisdiction,
    court_level: Option<&str>,
    as_of: Option<i32>,
) -> Resolution {
    let state_actor = jurisdictions::is_state_forum(court_level) && jur.kind != ForumKind::Federal;
    let incorporated = match clause.incorporation {
        Incorporation::Incorporated | Incorporation::NotApplicable => true,
        Incorporation::Unsettled | Incorporation::NotIncorporated => false,
    };
    let skipped_unincorporated =
        state_actor && clause.incorporation == Incorporation::NotIncorporated;

    let mut notes = Vec::new();
    let related: Vec<&Holding> = holdings::holdings_for_clause(clause.id);
    let overruled: Vec<HoldingView> = related
        .iter()
        .filter(|h| h.authority == Authority::Overruled || h.superseded_by.is_some())
        .map(|h| HoldingView::from(*h))
        .collect();

    let scotus: Vec<&Holding> = related
        .iter()
        .copied()
        .filter(|h| h.court_kind == CourtKind::Scotus && is_live(h, as_of))
        .collect();
    let binding = scotus
        .iter()
        .max_by_key(|h| h.year)
        .map(|h| HoldingView::from(*h));

    let circuit_holding = related
        .iter()
        .copied()
        .filter(|h| {
            h.court_kind == CourtKind::Circuit && h.court_id == jur.circuit && is_live(h, as_of)
        })
        .max_by_key(|h| h.year)
        .map(HoldingView::from);

    let split = holdings::SPLITS.iter().find(|s| s.clause_id == clause.id);
    let split_view = split.map(|s| {
        let forum_side = if s.side_a_circuits.contains(&jur.circuit) {
            Some("a")
        } else if s.side_b_circuits.contains(&jur.circuit) {
            Some("b")
        } else {
            None
        };
        SplitView {
            id: s.id,
            question: s.question,
            side_a_circuits: s.side_a_circuits,
            side_a_view: s.side_a_view,
            side_b_circuits: s.side_b_circuits,
            side_b_view: s.side_b_view,
            notes: s.notes,
            forum_side,
        }
    });

    let analog = if state_actor {
        holdings::analogs_for(jur.code)
            .into_iter()
            .find(|a| a.clause_id == clause.id)
            .map(|a| analog_view(a, &split_view))
    } else {
        None
    };

    if skipped_unincorporated {
        notes.push(format!(
            "{} is not incorporated against the states; skipped for a state-actor forum.",
            clause.label
        ));
        return Resolution {
            clause_id: clause.id,
            clause_label: clause.label,
            jurisdiction: jur.code,
            jurisdiction_name: jur.name,
            circuit: jur.circuit,
            state_actor,
            incorporated: false,
            skipped_unincorporated: true,
            status: ResolveStatus::Inapplicable,
            binding,
            circuit_holding,
            overruled,
            split: split_view,
            state_analog: analog,
            notes,
        };
    }

    if let Some(ref s) = split_view {
        notes.push(format!(
            "Circuit split on residual question: {}",
            s.question
        ));
        notes.push(s.notes.to_string());
    }
    if let Some(ref a) = analog {
        if a.state_above_federal {
            notes.push(format!(
                "State analog {} may raise the floor above the federal Constitution; it never shrinks federal rights.",
                a.state_citation
            ));
        }
    }
    notes.push(
        "Holdings snapshot is curated and incomplete. Output is an attorney-review lead, not a legal conclusion.".into(),
    );

    // SCOTUS supplies a floor when present. A documented circuit split on a
    // residual question makes the *application* unsettled without dropping the floor.
    let status = match (
        binding.is_some(),
        split_view.is_some(),
        circuit_holding.is_some(),
    ) {
        (true, false, _) => ResolveStatus::Controlling,
        (true, true, _) => {
            notes.push(
                "SCOTUS supplies a controlling floor; a documented circuit split remains on a residual question.".into(),
            );
            ResolveStatus::Unsettled
        }
        (false, true, _) => ResolveStatus::Unsettled,
        (false, false, true) => ResolveStatus::CircuitBinding,
        _ => ResolveStatus::NoHolding,
    };

    Resolution {
        clause_id: clause.id,
        clause_label: clause.label,
        jurisdiction: jur.code,
        jurisdiction_name: jur.name,
        circuit: jur.circuit,
        state_actor,
        incorporated,
        skipped_unincorporated: false,
        status,
        binding,
        circuit_holding,
        overruled,
        split: split_view,
        state_analog: analog,
        notes,
    }
}

fn analog_view(a: StateAnalog, _split: &Option<SplitView>) -> AnalogView {
    AnalogView {
        code: a.code,
        clause_id: a.clause_id,
        state_citation: a.state_citation,
        relation: a.relation,
        more_protective: a.more_protective,
        notes: a.notes,
        state_above_federal: a.more_protective == Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(clause: &str, jur: &str, level: &str) -> ResolveQuery {
        ResolveQuery {
            clause_id: clause.into(),
            jurisdiction: jur.into(),
            court_level: Some(level.into()),
            as_of_year: None,
        }
    }

    #[test]
    fn scotus_floor_present_for_fourth() {
        let r = resolve(&q("amend.04.search_seizure", "TX", "superior")).unwrap();
        assert_eq!(r.binding.as_ref().unwrap().id, "carpenter");
        assert!(r.overruled.iter().any(|h| h.id == "wolf"));
        assert!(!r.skipped_unincorporated);
    }

    #[test]
    fn circuit_split_is_unsettled_with_both_sides() {
        let r = resolve(&q("amend.14.due_process", "CA", "superior")).unwrap();
        assert_eq!(r.status, ResolveStatus::Unsettled);
        let s = r.split.unwrap();
        assert!(!s.side_a_circuits.is_empty());
        assert!(!s.side_b_circuits.is_empty());
        assert_eq!(s.forum_side, Some("a"));
        assert_eq!(r.circuit, "CA9");
    }

    #[test]
    fn california_analog_can_raise_the_floor() {
        let r = resolve(&q("amend.04.search_seizure", "CA", "superior")).unwrap();
        let a = r.state_analog.unwrap();
        assert!(a.state_above_federal);
        assert!(a.state_citation.contains("art. I"));
    }

    #[test]
    fn florida_search_is_lockstep() {
        let r = resolve(&q("amend.04.search_seizure", "FL", "superior")).unwrap();
        let analog = r.state_analog.unwrap();
        assert_eq!(analog.relation, Relation::Lockstep);
        assert!(!analog.state_above_federal);
    }

    #[test]
    fn unincorporated_grand_jury_skipped_for_state_forum() {
        let r = resolve(&q("amend.05.grand_jury", "CA", "superior")).unwrap();
        assert!(r.skipped_unincorporated);
        assert_eq!(r.status, ResolveStatus::Inapplicable);
        let fed = resolve(&q("amend.05.grand_jury", "CA", "federal_district")).unwrap();
        assert!(!fed.skipped_unincorporated);
    }

    #[test]
    fn unknown_jurisdiction_is_an_error() {
        assert!(resolve(&q("amend.04.search_seizure", "XX", "superior")).is_err());
    }
}
