//! Equal-application sentencing advocacy.
//!
//! After a conviction, counsel seeks the statutory maximum the same law
//! provides — including life imprisonment when 18 U.S.C. §§ 241, 242, or
//! 1512 authorize it (death resulting, or the statute's listed predicates).
//! The engine does not file the memo. It does advocate.
#![forbid(unsafe_code)]

use serde::Serialize;

use crate::statutes::Statute;

#[derive(Debug, Clone, Serialize)]
pub struct StatuteAdvocacy {
    pub citation: &'static str,
    pub kind: &'static str,
    pub applicable_maximum: &'static str,
    pub advocated: &'static str,
    pub life_available: bool,
    pub predicate: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentenceAdvocacy {
    pub death_resulted: bool,
    pub bodily_injury: bool,
    pub life_available: bool,
    pub advocated_sentence: String,
    pub equal_application: &'static str,
    pub by_statute: Vec<StatuteAdvocacy>,
}

pub const EQUAL_APPLICATION: &str = "The official is subject to the same criminal law applied to everyone else. If convicted, counsel shall seek the statutory maximum those titles provide — including life imprisonment where the statute authorizes it. Color of law is an element of the offense, not a shield from the sentence.";

pub fn for_statute(
    statute: &Statute,
    death_resulted: bool,
    bodily_injury: bool,
) -> Option<StatuteAdvocacy> {
    if statute.kind != "criminal" {
        return None;
    }
    let (applicable_maximum, advocated, life_available, predicate) = match statute.citation {
        "18 U.S.C. § 242" if death_resulted => (
            "any term of years or life (death resulting)",
            "life imprisonment",
            true,
            "death resulting from the deprivation of rights",
        ),
        "18 U.S.C. § 242" if bodily_injury => (
            "up to 10 years (bodily injury)",
            "10 years",
            false,
            "bodily injury",
        ),
        "18 U.S.C. § 242" => (
            "up to 1 year",
            "1 year",
            false,
            "color-of-law deprivation without the injury or death enhancer",
        ),
        "18 U.S.C. § 241" if death_resulted => (
            "any term of years or life (death resulting)",
            "life imprisonment",
            true,
            "death resulting from the conspiracy against rights",
        ),
        "18 U.S.C. § 241" => (
            "up to 10 years",
            "10 years",
            false,
            "conspiracy against rights without the death enhancer",
        ),
        "18 U.S.C. § 1512" if death_resulted => (
            "any term of years or life (death resulting)",
            "life imprisonment",
            true,
            "death resulting from witness/victim/informant tampering",
        ),
        "18 U.S.C. § 1512" => (
            "up to 20 years (typical subsection)",
            "20 years",
            false,
            "tampering without the death enhancer",
        ),
        "18 U.S.C. § 1621" | "18 U.S.C. § 1622" => {
            ("up to 5 years", "5 years", false, "perjury / subornation")
        }
        _ => (
            statute.statutory_maximum,
            "statutory maximum",
            false,
            "see catalog",
        ),
    };
    Some(StatuteAdvocacy {
        citation: statute.citation,
        kind: statute.kind,
        applicable_maximum,
        advocated,
        life_available,
        predicate,
    })
}

pub fn assemble(
    statutes: &[&Statute],
    death_resulted: bool,
    bodily_injury: bool,
) -> SentenceAdvocacy {
    let by_statute: Vec<StatuteAdvocacy> = statutes
        .iter()
        .filter_map(|s| for_statute(s, death_resulted, bodily_injury))
        .collect();
    let life_available = by_statute.iter().any(|s| s.life_available);
    let advocated_sentence = if life_available {
        "life imprisonment — the statutory maximum under the death-resulting color-of-law titles"
            .to_string()
    } else if let Some(top) = by_statute.iter().max_by_key(|s| rank(s.advocated)) {
        format!("{} ({})", top.advocated, top.citation)
    } else {
        "statutory maximum of each proven count".to_string()
    };
    SentenceAdvocacy {
        death_resulted,
        bodily_injury,
        life_available,
        advocated_sentence,
        equal_application: EQUAL_APPLICATION,
        by_statute,
    }
}

fn rank(advocated: &str) -> u32 {
    if advocated.contains("life") {
        100
    } else if advocated.contains("20") {
        20
    } else if advocated.contains("10") {
        10
    } else if advocated.contains("5") {
        5
    } else if advocated.contains("1") {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statutes::{self, STATUTES};

    fn statute(cite: &str) -> &'static crate::statutes::Statute {
        STATUTES.iter().find(|s| s.citation == cite).unwrap()
    }

    #[test]
    fn death_resulting_color_of_law_advocates_life() {
        let mapped = statutes::for_finding_types(&["due_process".into()]);
        let adv = assemble(&mapped, true, false);
        assert!(adv.life_available);
        assert!(adv.advocated_sentence.contains("life imprisonment"));
        assert!(adv
            .by_statute
            .iter()
            .any(|s| s.citation.contains("242") && s.life_available));
        assert!(adv
            .by_statute
            .iter()
            .any(|s| s.citation.contains("241") && s.life_available));
    }

    #[test]
    fn brady_without_death_does_not_unlock_life() {
        let mapped = statutes::for_finding_types(&["brady".into()]);
        let adv = assemble(&mapped, false, false);
        assert!(!adv.life_available);
        assert!(!adv.advocated_sentence.contains("life imprisonment"));
        let s242 = for_statute(statute("18 U.S.C. § 242"), false, false).unwrap();
        assert_eq!(s242.advocated, "1 year");
    }

    #[test]
    fn bodily_injury_raises_242_to_ten() {
        let s = for_statute(statute("18 U.S.C. § 242"), false, true).unwrap();
        assert_eq!(s.advocated, "10 years");
        assert!(!s.life_available);
    }
}
