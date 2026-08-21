//! All 50 states, DC, and inhabited territories — selectable national forums.
//! Circuit mapping is geographic (Federal Circuit is specialized, not a state forum).
#![forbid(unsafe_code)]

use serde::Serialize;

pub const SNAPSHOT_ID: &str = "criminal-procedure-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForumKind {
    State,
    District,
    Territory,
    Federal,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Jurisdiction {
    pub code: &'static str,
    pub name: &'static str,
    pub kind: ForumKind,
    pub circuit: &'static str,
    pub selectable: bool,
    pub sort_order: u16,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Circuit {
    pub id: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CourtLevelOpt {
    pub id: &'static str,
    pub label: &'static str,
    pub forum: &'static str,
}

pub const CIRCUITS: &[Circuit] = &[
    Circuit {
        id: "CA1",
        name: "U.S. Court of Appeals for the First Circuit",
    },
    Circuit {
        id: "CA2",
        name: "U.S. Court of Appeals for the Second Circuit",
    },
    Circuit {
        id: "CA3",
        name: "U.S. Court of Appeals for the Third Circuit",
    },
    Circuit {
        id: "CA4",
        name: "U.S. Court of Appeals for the Fourth Circuit",
    },
    Circuit {
        id: "CA5",
        name: "U.S. Court of Appeals for the Fifth Circuit",
    },
    Circuit {
        id: "CA6",
        name: "U.S. Court of Appeals for the Sixth Circuit",
    },
    Circuit {
        id: "CA7",
        name: "U.S. Court of Appeals for the Seventh Circuit",
    },
    Circuit {
        id: "CA8",
        name: "U.S. Court of Appeals for the Eighth Circuit",
    },
    Circuit {
        id: "CA9",
        name: "U.S. Court of Appeals for the Ninth Circuit",
    },
    Circuit {
        id: "CA10",
        name: "U.S. Court of Appeals for the Tenth Circuit",
    },
    Circuit {
        id: "CA11",
        name: "U.S. Court of Appeals for the Eleventh Circuit",
    },
    Circuit {
        id: "CADC",
        name: "U.S. Court of Appeals for the District of Columbia Circuit",
    },
    Circuit {
        id: "CAFC",
        name: "U.S. Court of Appeals for the Federal Circuit",
    },
];

pub const COURT_LEVELS: &[CourtLevelOpt] = &[
    CourtLevelOpt {
        id: "superior",
        label: "State trial court",
        forum: "state",
    },
    CourtLevelOpt {
        id: "state_appellate",
        label: "State intermediate appellate court",
        forum: "state",
    },
    CourtLevelOpt {
        id: "state_supreme",
        label: "State supreme court",
        forum: "state",
    },
    CourtLevelOpt {
        id: "federal_district",
        label: "U.S. District Court",
        forum: "federal",
    },
    CourtLevelOpt {
        id: "federal_circuit",
        label: "U.S. Court of Appeals",
        forum: "federal",
    },
    CourtLevelOpt {
        id: "scotus",
        label: "Supreme Court of the United States",
        forum: "federal",
    },
];

macro_rules! j {
    ($code:expr, $name:expr, $kind:expr, $circuit:expr, $sort:expr) => {
        Jurisdiction {
            code: $code,
            name: $name,
            kind: $kind,
            circuit: $circuit,
            selectable: true,
            sort_order: $sort,
        }
    };
}

/// Selectable forums: 50 states, DC, five inhabited territories, plus a federal umbrella.
pub const JURISDICTIONS: &[Jurisdiction] = &[
    j!("AL", "Alabama", ForumKind::State, "CA11", 1),
    j!("AK", "Alaska", ForumKind::State, "CA9", 2),
    j!("AZ", "Arizona", ForumKind::State, "CA9", 3),
    j!("AR", "Arkansas", ForumKind::State, "CA8", 4),
    j!("CA", "California", ForumKind::State, "CA9", 5),
    j!("CO", "Colorado", ForumKind::State, "CA10", 6),
    j!("CT", "Connecticut", ForumKind::State, "CA2", 7),
    j!("DE", "Delaware", ForumKind::State, "CA3", 8),
    j!("FL", "Florida", ForumKind::State, "CA11", 9),
    j!("GA", "Georgia", ForumKind::State, "CA11", 10),
    j!("HI", "Hawaii", ForumKind::State, "CA9", 11),
    j!("ID", "Idaho", ForumKind::State, "CA9", 12),
    j!("IL", "Illinois", ForumKind::State, "CA7", 13),
    j!("IN", "Indiana", ForumKind::State, "CA7", 14),
    j!("IA", "Iowa", ForumKind::State, "CA8", 15),
    j!("KS", "Kansas", ForumKind::State, "CA10", 16),
    j!("KY", "Kentucky", ForumKind::State, "CA6", 17),
    j!("LA", "Louisiana", ForumKind::State, "CA5", 18),
    j!("ME", "Maine", ForumKind::State, "CA1", 19),
    j!("MD", "Maryland", ForumKind::State, "CA4", 20),
    j!("MA", "Massachusetts", ForumKind::State, "CA1", 21),
    j!("MI", "Michigan", ForumKind::State, "CA6", 22),
    j!("MN", "Minnesota", ForumKind::State, "CA8", 23),
    j!("MS", "Mississippi", ForumKind::State, "CA5", 24),
    j!("MO", "Missouri", ForumKind::State, "CA8", 25),
    j!("MT", "Montana", ForumKind::State, "CA9", 26),
    j!("NE", "Nebraska", ForumKind::State, "CA8", 27),
    j!("NV", "Nevada", ForumKind::State, "CA9", 28),
    j!("NH", "New Hampshire", ForumKind::State, "CA1", 29),
    j!("NJ", "New Jersey", ForumKind::State, "CA3", 30),
    j!("NM", "New Mexico", ForumKind::State, "CA10", 31),
    j!("NY", "New York", ForumKind::State, "CA2", 32),
    j!("NC", "North Carolina", ForumKind::State, "CA4", 33),
    j!("ND", "North Dakota", ForumKind::State, "CA8", 34),
    j!("OH", "Ohio", ForumKind::State, "CA6", 35),
    j!("OK", "Oklahoma", ForumKind::State, "CA10", 36),
    j!("OR", "Oregon", ForumKind::State, "CA9", 37),
    j!("PA", "Pennsylvania", ForumKind::State, "CA3", 38),
    j!("RI", "Rhode Island", ForumKind::State, "CA1", 39),
    j!("SC", "South Carolina", ForumKind::State, "CA4", 40),
    j!("SD", "South Dakota", ForumKind::State, "CA8", 41),
    j!("TN", "Tennessee", ForumKind::State, "CA6", 42),
    j!("TX", "Texas", ForumKind::State, "CA5", 43),
    j!("UT", "Utah", ForumKind::State, "CA10", 44),
    j!("VT", "Vermont", ForumKind::State, "CA2", 45),
    j!("VA", "Virginia", ForumKind::State, "CA4", 46),
    j!("WA", "Washington", ForumKind::State, "CA9", 47),
    j!("WV", "West Virginia", ForumKind::State, "CA4", 48),
    j!("WI", "Wisconsin", ForumKind::State, "CA7", 49),
    j!("WY", "Wyoming", ForumKind::State, "CA10", 50),
    j!(
        "DC",
        "District of Columbia",
        ForumKind::District,
        "CADC",
        51
    ),
    j!("AS", "American Samoa", ForumKind::Territory, "CA9", 52),
    j!("GU", "Guam", ForumKind::Territory, "CA9", 53),
    j!(
        "MP",
        "Northern Mariana Islands",
        ForumKind::Territory,
        "CA9",
        54
    ),
    j!("PR", "Puerto Rico", ForumKind::Territory, "CA1", 55),
    j!("VI", "U.S. Virgin Islands", ForumKind::Territory, "CA3", 56),
    j!(
        "US",
        "United States (federal)",
        ForumKind::Federal,
        "CAFC",
        57
    ),
];

pub fn lookup(code: &str) -> Option<&'static Jurisdiction> {
    let needle = code.trim();
    JURISDICTIONS
        .iter()
        .find(|j| j.code.eq_ignore_ascii_case(needle) || j.name.eq_ignore_ascii_case(needle))
}

pub fn states() -> impl Iterator<Item = &'static Jurisdiction> {
    JURISDICTIONS.iter().filter(|j| j.kind == ForumKind::State)
}

pub fn is_state_forum(court_level: Option<&str>) -> bool {
    match court_level {
        Some(s) => {
            let s = s.trim().to_ascii_lowercase();
            !matches!(
                s.as_str(),
                "federal_district" | "federal_circuit" | "scotus"
            )
        }
        None => true,
    }
}

pub fn circuit_name(id: &str) -> Option<&'static str> {
    CIRCUITS.iter().find(|c| c.id == id).map(|c| c.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn fifty_states_selectable() {
        let states: Vec<_> = states().collect();
        assert_eq!(states.len(), 50);
        assert!(states.iter().all(|s| s.selectable));
        let codes: HashSet<_> = states.iter().map(|s| s.code).collect();
        assert_eq!(codes.len(), 50);
        assert!(codes.contains("CA"));
        assert!(codes.contains("WY"));
        assert!(codes.contains("HI"));
        assert!(codes.contains("AK"));
        assert!(lookup("ca").unwrap().circuit == "CA9");
        assert!(lookup("Alabama").unwrap().code == "AL");
    }

    #[test]
    fn thirteen_circuits_and_national_forums() {
        assert_eq!(CIRCUITS.len(), 13);
        assert!(lookup("DC").is_some());
        assert!(lookup("PR").is_some());
        assert!(is_state_forum(Some("superior")));
        assert!(!is_state_forum(Some("federal_district")));
    }
}
