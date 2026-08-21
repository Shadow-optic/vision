//! Fact-pattern screening: maps public-record case facts onto clauses, then resolves.
//! Hits are leads for attorney review — never auto-published findings.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::jurisdictions;
use crate::resolve::{self, Resolution, ResolveQuery, ResolveStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenInput {
    pub jurisdiction: String,
    pub court_level: Option<String>,
    pub plea_offered: Option<bool>,
    pub plea_accepted: Option<bool>,
    pub outcome: Option<String>,
    pub plea_offer_months: Option<i32>,
    pub sentence_months: Option<i32>,
    pub evidence_strength: Option<String>,
    pub defendant_race: Option<String>,
    pub charge_category: Option<String>,
    pub opinion_text: Option<String>,
    pub has_brady_gaps: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenHit {
    pub clause_id: &'static str,
    pub label: String,
    pub severity: Severity,
    pub matched: Vec<String>,
    pub resolution: Resolution,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenReport {
    pub jurisdiction: String,
    pub circuit: Option<String>,
    pub snapshot_id: &'static str,
    pub corpus_hash: String,
    pub authority: &'static str,
    pub hits: Vec<ScreenHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DerivedFeatures {
    pub jurisdiction_known: bool,
    pub jurisdiction: Option<String>,
    pub jurisdiction_name: Option<String>,
    pub circuit: Option<String>,
    pub amend_04_candidate: bool,
    pub amend_06_trial_right_pressure: bool,
    pub due_process_disclosure_review: bool,
    pub equal_protection_review: bool,
    pub authority: &'static str,
}

pub fn plea_sentence_ratio(input: &ScreenInput) -> Option<f64> {
    match (
        input.plea_offered,
        input.plea_accepted,
        input.outcome.as_deref(),
        input.plea_offer_months,
        input.sentence_months,
    ) {
        (Some(true), Some(false), Some("conviction"), Some(offer), Some(sent)) if sent > 0 => {
            Some(offer as f64 / sent as f64)
        }
        _ => None,
    }
}

pub fn derived_features(input: &ScreenInput) -> DerivedFeatures {
    let jur = jurisdictions::lookup(&input.jurisdiction);
    let ratio = plea_sentence_ratio(input);
    let trial_pressure = ratio.map(|r| r < 0.5).unwrap_or(false);
    let weak = input
        .evidence_strength
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("weak"));
    let conviction = input.outcome.as_deref() == Some("conviction");
    DerivedFeatures {
        jurisdiction_known: jur.is_some(),
        jurisdiction: jur.map(|j| j.code.to_string()),
        jurisdiction_name: jur.map(|j| j.name.to_string()),
        circuit: jur.map(|j| j.circuit.to_string()),
        amend_04_candidate: mentions_search(input.opinion_text.as_deref()),
        amend_06_trial_right_pressure: trial_pressure,
        due_process_disclosure_review: input.has_brady_gaps
            || mentions_nondisclosure(input.opinion_text.as_deref())
            || (weak && conviction),
        equal_protection_review: input.defendant_race.as_ref().is_some_and(|r| !r.is_empty())
            && input
                .charge_category
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("drug"))
            && conviction,
        authority: "advisory",
    }
}

/// Injects `constitution.*` into a TrustScript case-context object.
pub fn attach_features(ctx: &mut Value) {
    let Some(case) = ctx.get("case").cloned() else {
        return;
    };
    let input = input_from_case_json(&case);
    let feat = derived_features(&input);
    if let Some(obj) = ctx.as_object_mut() {
        obj.insert(
            "constitution".into(),
            json!({
                "jurisdiction_known": feat.jurisdiction_known,
                "jurisdiction": feat.jurisdiction,
                "jurisdiction_name": feat.jurisdiction_name,
                "circuit": feat.circuit,
                "amend_04": { "candidate": feat.amend_04_candidate },
                "amend_06": { "trial_right_pressure": feat.amend_06_trial_right_pressure },
                "amend_14": { "equal_protection_review": feat.equal_protection_review },
                "due_process_disclosure": feat.due_process_disclosure_review,
                "authority": feat.authority,
            }),
        );
    }
}

pub fn input_from_case_json(case: &Value) -> ScreenInput {
    ScreenInput {
        jurisdiction: case
            .get("jurisdiction")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        court_level: case
            .get("court_level")
            .and_then(Value::as_str)
            .map(str::to_string),
        plea_offered: case.get("plea_offered").and_then(Value::as_bool),
        plea_accepted: case.get("plea_accepted").and_then(Value::as_bool),
        outcome: case
            .get("outcome")
            .and_then(Value::as_str)
            .map(str::to_string),
        plea_offer_months: case
            .get("plea_offer_months")
            .and_then(Value::as_i64)
            .map(|n| n as i32),
        sentence_months: case
            .get("sentence_months")
            .and_then(Value::as_i64)
            .map(|n| n as i32),
        evidence_strength: case
            .get("evidence_strength")
            .and_then(Value::as_str)
            .map(str::to_string),
        defendant_race: case
            .get("defendant_race")
            .and_then(Value::as_str)
            .map(str::to_string),
        charge_category: case
            .get("charge_category")
            .and_then(Value::as_str)
            .map(str::to_string),
        opinion_text: None,
        has_brady_gaps: false,
    }
}

fn mentions_search(text: Option<&str>) -> bool {
    let Some(t) = text else {
        return false;
    };
    let l = t.to_ascii_lowercase();
    l.contains("suppress") || l.contains("search") || l.contains("seizure") || l.contains("warrant")
}

fn mentions_nondisclosure(text: Option<&str>) -> bool {
    let Some(t) = text else {
        return false;
    };
    let l = t.to_ascii_lowercase();
    l.contains("not produced")
        || l.contains("never disclosed")
        || l.contains("was not produced")
        || l.contains("chain of custody documentation was not")
}

pub fn screen(input: &ScreenInput) -> ScreenReport {
    let feat = derived_features(input);
    let mut candidates: Vec<(&str, Severity, Vec<String>)> = Vec::new();

    if feat.amend_06_trial_right_pressure {
        let mut m =
            vec!["plea offer rejected, conviction followed, plea/sentence ratio < 0.5".into()];
        if let Some(r) = plea_sentence_ratio(input) {
            m.push(format!("plea_sentence_ratio={r:.3}"));
        }
        if input.evidence_strength.as_deref() == Some("weak") {
            m.push("evidence_strength=weak".into());
        }
        candidates.push(("amend.06.jury", Severity::High, m.clone()));
        candidates.push(("amend.06.counsel", Severity::High, m));
    }
    if feat.due_process_disclosure_review {
        let mut m = Vec::new();
        if input.has_brady_gaps {
            m.push("brady reconciliation reported gaps".into());
        }
        if mentions_nondisclosure(input.opinion_text.as_deref()) {
            m.push("opinion text references undisclosed or unproduced evidence".into());
        }
        if input.evidence_strength.as_deref() == Some("weak") {
            m.push("weak evidence_strength with conviction".into());
        }
        if m.is_empty() {
            m.push("due-process disclosure review triggered".into());
        }
        candidates.push(("amend.14.due_process", Severity::High, m.clone()));
        candidates.push(("amend.05.due_process", Severity::Medium, m));
    }
    if feat.amend_04_candidate {
        candidates.push((
            "amend.04.search_seizure",
            Severity::Medium,
            vec!["opinion or docket language indicates a suppression / search issue".into()],
        ));
    }
    if feat.equal_protection_review {
        candidates.push((
            "amend.14.equal_protection",
            Severity::Medium,
            vec![
                "defendant_race present, drug charge, conviction — selective-prosecution / Batson-class review lead".into(),
            ],
        ));
    }

    let mut hits = Vec::new();
    let mut any_unsettled = false;
    for (clause_id, severity, matched) in candidates {
        let q = ResolveQuery {
            clause_id: clause_id.into(),
            jurisdiction: input.jurisdiction.clone(),
            court_level: input.court_level.clone(),
            as_of_year: None,
        };
        let Ok(resolution) = resolve::resolve(&q) else {
            continue;
        };
        if resolution.skipped_unincorporated {
            continue;
        }
        if resolution.status == ResolveStatus::Unsettled {
            any_unsettled = true;
        }
        hits.push(ScreenHit {
            clause_id: resolution.clause_id,
            label: format!("{} — {}", resolution.clause_label, resolution.clause_id),
            severity,
            matched,
            resolution,
        });
    }

    ScreenReport {
        jurisdiction: feat
            .jurisdiction
            .unwrap_or_else(|| input.jurisdiction.clone()),
        circuit: feat.circuit,
        snapshot_id: crate::jurisdictions::SNAPSHOT_ID,
        corpus_hash: crate::corpus::corpus_hash(),
        authority: if any_unsettled {
            "unsettled"
        } else {
            "controlling"
        },
        hits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> ScreenInput {
        ScreenInput {
            jurisdiction: "CA".into(),
            court_level: Some("superior".into()),
            plea_offered: Some(true),
            plea_accepted: Some(false),
            outcome: Some("conviction".into()),
            plea_offer_months: Some(12),
            sentence_months: Some(36),
            evidence_strength: Some("weak".into()),
            defendant_race: Some("Black".into()),
            charge_category: Some("drug".into()),
            opinion_text: Some(
                "The defendant moved to suppress evidence. Body-worn camera footage was referenced \
                 but the chain of custody documentation was not produced. A 911 call recording was never disclosed."
                    .into(),
            ),
            has_brady_gaps: true,
        }
    }

    #[test]
    fn demo_facts_attach_sixth_and_due_process() {
        let r = screen(&demo());
        assert_eq!(r.circuit.as_deref(), Some("CA9"));
        let ids: Vec<_> = r.hits.iter().map(|h| h.clause_id).collect();
        assert!(ids.contains(&"amend.06.jury"));
        assert!(ids.contains(&"amend.14.due_process"));
        assert!(ids.contains(&"amend.04.search_seizure"));
        assert!(r.hits.iter().any(|h| h.severity == Severity::High));
    }

    #[test]
    fn attach_features_nested_paths() {
        let mut ctx = json!({"case": {
            "jurisdiction": "CA",
            "plea_offered": true,
            "plea_accepted": false,
            "outcome": "conviction",
            "plea_offer_months": 12,
            "sentence_months": 36
        }});
        attach_features(&mut ctx);
        assert_eq!(ctx["constitution"]["circuit"], "CA9");
        assert_eq!(
            ctx["constitution"]["amend_06"]["trial_right_pressure"],
            true
        );
    }
}
