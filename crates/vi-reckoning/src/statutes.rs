//! Statute catalog for attorney research — not charging decisions.
//! Criminal elements (especially willfulness) cannot be found by software.
#![forbid(unsafe_code)]

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Statute {
    pub citation: &'static str,
    pub title: &'static str,
    pub kind: &'static str,
    pub elements: &'static [&'static str],
    pub statutory_maximum: &'static str,
    pub finding_types: &'static [&'static str],
    pub research_note: &'static str,
}

pub const STATUTES: &[Statute] = &[
    Statute {
        citation: "18 U.S.C. § 242",
        title: "Deprivation of rights under color of law",
        kind: "criminal",
        elements: &[
            "acted under color of law",
            "deprived a person of a right protected by the Constitution or laws of the United States",
            "acted willfully",
        ],
        statutory_maximum: "Fine and/or imprisonment up to 1 year; up to 10 years if bodily injury or certain dangerous weapons/threats; any term of years or life if death results (or certain kidnapping/aggravated sexual-abuse predicates).",
        finding_types: &["brady", "giglio", "due_process", "witness_subornation"],
        research_note: "Willfulness is a jury question. A substantiated Brady finding is a factual predicate for counsel, not a completed § 242 offense.",
    },
    Statute {
        citation: "18 U.S.C. § 241",
        title: "Conspiracy against rights",
        kind: "criminal",
        elements: &[
            "two or more persons conspired",
            "to injure, oppress, threaten, or intimidate any person in the free exercise of a right or privilege secured by the Constitution or laws of the United States",
        ],
        statutory_maximum: "Fine and/or imprisonment up to 10 years; any term of years or life if death results (or certain predicates).",
        finding_types: &["brady", "giglio", "due_process", "witness_subornation"],
        research_note: "Requires evidence of agreement. Office custom alone is not a conspiracy.",
    },
    Statute {
        citation: "18 U.S.C. § 1621",
        title: "Perjury generally",
        kind: "criminal",
        elements: &[
            "took an oath before a competent tribunal, officer, or person",
            "made a statement willfully that the declarant did not believe to be true",
            "the statement was material",
        ],
        statutory_maximum: "Fine and/or imprisonment up to 5 years (or up to 8 years when committed in connection with certain offenses).",
        finding_types: &["witness_subornation"],
        research_note: "Requires a specific false statement under oath. Opinion language is not itself perjury.",
    },
    Statute {
        citation: "18 U.S.C. § 1622",
        title: "Subornation of perjury",
        kind: "criminal",
        elements: &[
            "procured another to commit perjury",
            "the procured testimony was in fact perjury",
        ],
        statutory_maximum: "Fine and/or imprisonment up to 5 years (or up to 8 years when committed in connection with certain offenses).",
        finding_types: &["witness_subornation"],
        research_note: "Both procurement and completed perjury must be proven.",
    },
    Statute {
        citation: "18 U.S.C. § 1512",
        title: "Tampering with a witness, victim, or an informant",
        kind: "criminal",
        elements: &[
            "knowingly used intimidation, threats, corrupt persuasion, or misleading conduct",
            "with intent to influence, delay, or prevent testimony, or to cause a person to withhold/destroy objects in an official proceeding",
        ],
        statutory_maximum: "Varies by subsection; typically up to 20 years. Death-resulting provisions carry any term of years or life.",
        finding_types: &["witness_subornation"],
        research_note: "Subsection selection is fact-specific. Do not charge from a gap report alone.",
    },
    Statute {
        citation: "42 U.S.C. § 1983",
        title: "Civil action for deprivation of rights",
        kind: "civil",
        elements: &[
            "person acted under color of state law",
            "deprived plaintiff of a right secured by the Constitution or federal law",
        ],
        statutory_maximum: "Civil damages, injunction, and attorney fees (42 U.S.C. § 1988). Not a criminal sentence.",
        finding_types: &["brady", "giglio", "batson", "discovery", "due_process", "witness_subornation"],
        research_note: "Subject to immunity doctrines. Absolute prosecutorial immunity may bar damages for advocacy functions (Imbler v. Pachtman, 424 U.S. 409 (1976)); investigative/administrative acts may be qualified only (Buckley v. Fitzsimmons, 509 U.S. 259 (1993)).",
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct ImmunityNote {
    pub doctrine: &'static str,
    pub leading_case: &'static str,
    pub scope: &'static str,
    pub recognized_limits: &'static str,
}

pub const IMMUNITY: &[ImmunityNote] = &[
    ImmunityNote {
        doctrine: "Absolute prosecutorial immunity",
        leading_case: "Imbler v. Pachtman, 424 U.S. 409 (1976)",
        scope: "Damages actions under § 1983 for conduct intimately associated with the judicial phase of the criminal process (initiating a prosecution, presenting the State's case).",
        recognized_limits: "Does not bar criminal prosecution, bar discipline, or injunctive relief. Does not cover investigative or administrative acts. Does not legalize the underlying conduct.",
    },
    ImmunityNote {
        doctrine: "Qualified immunity (officers)",
        leading_case: "Harlow v. Fitzgerald, 457 U.S. 800 (1982); Pearson v. Callahan, 555 U.S. 223 (2009)",
        scope: "Shields officials from damages unless they violated a statutory or constitutional right that was clearly established at the time.",
        recognized_limits: "Fact-specific; does not apply to injunctive claims in the same way; municipal Monell liability is a separate track.",
    },
    ImmunityNote {
        doctrine: "Judicial immunity",
        leading_case: "Pierson v. Ray, 386 U.S. 547 (1967); Stump v. Sparkman, 435 U.S. 349 (1978)",
        scope: "Absolute immunity for judicial acts within jurisdiction.",
        recognized_limits: "Acts in the complete absence of all jurisdiction are not immune. Administrative acts may be qualified only. Criminal and disciplinary processes remain available where the law provides.",
    },
];

pub fn for_finding_types(types: &[String]) -> Vec<&'static Statute> {
    STATUTES
        .iter()
        .filter(|s| s.finding_types.iter().any(|t| types.iter().any(|x| x == t)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_core_titles() {
        let cites: Vec<_> = STATUTES.iter().map(|s| s.citation).collect();
        assert!(cites.contains(&"18 U.S.C. § 242"));
        assert!(cites.contains(&"18 U.S.C. § 241"));
        assert!(cites.contains(&"42 U.S.C. § 1983"));
    }

    #[test]
    fn brady_maps_to_civil_and_criminal_research() {
        let mapped = for_finding_types(&["brady".into()]);
        assert!(mapped.iter().any(|s| s.citation.contains("242")));
        assert!(mapped.iter().any(|s| s.citation.contains("1983")));
        assert!(mapped.iter().all(|s| s.research_note.len() > 20));
    }

    #[test]
    fn immunity_notes_preserve_due_process() {
        assert!(IMMUNITY
            .iter()
            .any(|n| n.doctrine.contains("prosecutorial")));
        assert!(IMMUNITY.iter().all(|n| !n.recognized_limits.is_empty()));
    }
}
