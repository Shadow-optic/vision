//! Turning a source court id into a forum the rest of the stack understands.
//!
//! Feeds identify courts ("cand", "delch", "ca9"); constitutional screening
//! needs a forum code from [`vi_constitution::jurisdictions`] plus a court
//! level. Every mapping records the method that produced it, because a court
//! silently mapped to the wrong forum would put a case under the wrong body of
//! law — and later, potentially, under the wrong statute.
#![forbid(unsafe_code)]

use vi_constitution::jurisdictions::{ForumKind, JURISDICTIONS};

/// Court levels, matching `vi_constitution::jurisdictions::COURT_LEVELS`.
pub const SCOTUS: &str = "scotus";
pub const FEDERAL_CIRCUIT: &str = "federal_circuit";
pub const FEDERAL_DISTRICT: &str = "federal_district";
pub const STATE_SUPREME: &str = "state_supreme";
pub const STATE_APPELLATE: &str = "state_appellate";
pub const STATE_TRIAL: &str = "state_trial";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// Forum code (`CA`, `DE`, `US`, ...), or `None` when nothing is defensible.
    pub jurisdiction: Option<String>,
    pub court_level: Option<String>,
    /// How the jurisdiction was derived. Persisted so a bad mapping is auditable.
    pub method: &'static str,
}

impl Mapping {
    fn unknown() -> Self {
        Self {
            jurisdiction: None,
            court_level: None,
            method: "unmapped",
        }
    }

    pub fn is_resolved(&self) -> bool {
        self.jurisdiction.is_some()
    }
}

/// Federal district ids encode the state: `cand`, `nysd`, `txed`, `dcd`.
/// Bankruptcy courts add a `b`: `canb`. Anything else is not a district id.
fn state_from_federal_id(court_id: &str) -> Option<&'static str> {
    let id = court_id.trim().to_ascii_lowercase();
    if id.len() < 3 || id.len() > 5 || !id.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let (prefix, rest) = id.split_at(2);
    let rest = rest
        .strip_suffix('d')
        .or_else(|| rest.strip_suffix('b'))
        .or_else(|| rest.strip_suffix("bc"))?;
    if !matches!(rest, "" | "n" | "s" | "e" | "w" | "m" | "c") {
        return None;
    }
    code_for(prefix)
}

/// Exact match against a forum code, case-insensitively.
fn code_for(candidate: &str) -> Option<&'static str> {
    JURISDICTIONS
        .iter()
        .find(|j| j.code.eq_ignore_ascii_case(candidate))
        .map(|j| j.code)
}

/// Longest forum name appearing in `haystack`. Longest-first so "West
/// Virginia" is never resolved as Virginia.
fn state_from_name(haystack: &str) -> Option<&'static str> {
    let hay = haystack.to_ascii_lowercase();
    let mut names: Vec<_> = JURISDICTIONS
        .iter()
        .filter(|j| j.kind != ForumKind::Federal)
        .collect();
    names.sort_by_key(|j| std::cmp::Reverse(j.name.len()));
    names
        .into_iter()
        .find(|j| {
            let name = j.name.to_ascii_lowercase();
            // Court names drop the national prefix: the Virgin Islands court
            // is "Supreme Court of the Virgin Islands", not "U.S. Virgin
            // Islands".
            let bare = name.trim_start_matches("u.s. ").to_string();
            contains_word(&hay, &name) || contains_word(&hay, &bare)
        })
        .map(|j| j.code)
}

/// Substring match on word boundaries: "Indiana" must not match "Indianapolis".
fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = hay.as_bytes();
    let mut from = 0;
    while let Some(rel) = hay[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == hay.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
        if from >= hay.len() {
            break;
        }
    }
    false
}

/// Court level from the source's own classification code. Territorial courts
/// (`T*`) occupy the same three tiers as state courts.
fn state_level(source_class: &str) -> Option<&'static str> {
    match source_class {
        "S" | "TS" => Some(STATE_SUPREME),
        "SA" | "TA" => Some(STATE_APPELLATE),
        "ST" | "SS" | "SAG" | "TT" | "TSP" => Some(STATE_TRIAL),
        _ => None,
    }
}

/// Derive a forum from one court record.
///
/// `source_class` is the feed's own classification (`F`, `FD`, `FB`, `FS`,
/// `S`, `SA`, `ST`, ...). `full_name` and `citation_string` are used as
/// evidence for state courts, where the id is not a reliable code.
pub fn derive(
    court_id: &str,
    source_class: Option<&str>,
    full_name: &str,
    citation_string: Option<&str>,
) -> Mapping {
    let id = court_id.trim().to_ascii_lowercase();
    let class = source_class.unwrap_or("").trim().to_ascii_uppercase();

    if id == SCOTUS || full_name.eq_ignore_ascii_case("Supreme Court of the United States") {
        return Mapping {
            jurisdiction: Some("US".into()),
            court_level: Some(SCOTUS.into()),
            method: "scotus",
        };
    }

    let looks_like_circuit = id.starts_with("ca")
        && id.len() <= 4
        && !id[2..].is_empty()
        && id[2..].chars().all(|c| c.is_ascii_digit());

    if class.starts_with('F') || class.starts_with("MA") || looks_like_circuit {
        // Circuits span states, so the forum is the federal umbrella.
        let is_circuit =
            class == "F" || matches!(id.as_str(), "cafc" | "cadc" | "cavc") || looks_like_circuit;
        if is_circuit {
            return Mapping {
                jurisdiction: Some("US".into()),
                court_level: Some(FEDERAL_CIRCUIT.into()),
                method: "federal_circuit",
            };
        }

        // District and bankruptcy courts sit in one state, which matters: a
        // federal case in California is screened under Ninth Circuit law.
        if matches!(class.as_str(), "FD" | "FB") {
            if let Some(code) = state_from_federal_id(&id) {
                return Mapping {
                    jurisdiction: Some(code.into()),
                    court_level: Some(FEDERAL_DISTRICT.into()),
                    method: if class == "FB" {
                        "federal_bankruptcy_id"
                    } else {
                        "federal_district_id"
                    },
                };
            }
            if let Some(code) = state_from_name(full_name) {
                return Mapping {
                    jurisdiction: Some(code.into()),
                    court_level: Some(FEDERAL_DISTRICT.into()),
                    method: "federal_district_name",
                };
            }
        }

        // Everything else federal — bankruptcy appellate panels, the special
        // courts and boards, and the military appellate courts — is a federal
        // forum with no state analog. The level vocabulary has no separate
        // slot for them, so they are recorded as federal appellate bodies and
        // the method says which kind they really are. That distinction is
        // preserved rather than flattened, because screening a court-martial
        // under a state constitution would be nonsense.
        let method = match class.as_str() {
            "FBP" => "federal_bankruptcy_appellate",
            "FS" => "federal_special",
            c if c.starts_with("MA") => "military_appellate",
            _ => "federal_other",
        };
        return Mapping {
            jurisdiction: Some("US".into()),
            court_level: Some(FEDERAL_CIRCUIT.into()),
            method,
        };
    }

    // Tribal courts are sovereign forums of their own. There is no code for
    // them in the corpus, and inventing one — or filing a tribal judgment
    // under the law of the surrounding state — would misstate whose law
    // governs. They are recorded and left unscreened, deliberately.
    if class.starts_with("TR") {
        return Mapping {
            jurisdiction: None,
            court_level: None,
            method: "tribal_sovereign",
        };
    }

    // State and territorial courts. The id is not a reliable code, so the
    // court's own name and citation string are the evidence.
    if class.starts_with('S') || class.starts_with('T') || class.is_empty() {
        let level = state_level(&class);
        if let Some(code) = state_from_name(full_name) {
            return Mapping {
                jurisdiction: Some(code.into()),
                court_level: level.map(str::to_string),
                method: "state_name",
            };
        }
        if let Some(code) = citation_string.and_then(state_from_name) {
            return Mapping {
                jurisdiction: Some(code.into()),
                court_level: level.map(str::to_string),
                method: "state_citation",
            };
        }
        // A few ids are the code itself ("ny"); most are not ("cal", "wash").
        if let Some(code) = code_for(&id) {
            return Mapping {
                jurisdiction: Some(code.into()),
                court_level: level.map(str::to_string),
                method: "state_id",
            };
        }
    }

    Mapping::unknown()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scotus_is_federal() {
        let m = derive("scotus", Some("F"), "Supreme Court of the United States", None);
        assert_eq!(m.jurisdiction.as_deref(), Some("US"));
        assert_eq!(m.court_level.as_deref(), Some(SCOTUS));
    }

    #[test]
    fn circuits_are_the_federal_umbrella() {
        for id in ["ca1", "ca9", "ca11", "cafc", "cadc"] {
            let m = derive(id, Some("F"), "Court of Appeals", None);
            assert_eq!(m.jurisdiction.as_deref(), Some("US"), "{id}");
            assert_eq!(m.court_level.as_deref(), Some(FEDERAL_CIRCUIT), "{id}");
        }
    }

    #[test]
    fn district_ids_carry_their_state() {
        let cases = [
            ("cand", "CA"),
            ("nysd", "NY"),
            ("txed", "TX"),
            ("dcd", "DC"),
            ("mad", "MA"),
            ("canb", "CA"),
        ];
        for (id, expected) in cases {
            let m = derive(id, Some("FD"), "United States District Court", None);
            assert_eq!(m.jurisdiction.as_deref(), Some(expected), "{id}");
            assert_eq!(m.court_level.as_deref(), Some(FEDERAL_DISTRICT), "{id}");
        }
    }

    #[test]
    fn state_courts_resolve_from_their_name() {
        let m = derive("delch", Some("SS"), "Court of Chancery of Delaware", None);
        assert_eq!(m.jurisdiction.as_deref(), Some("DE"));
        assert_eq!(m.court_level.as_deref(), Some(STATE_TRIAL));

        let m = derive("cal", Some("S"), "Supreme Court of California", None);
        assert_eq!(m.jurisdiction.as_deref(), Some("CA"));
        assert_eq!(m.court_level.as_deref(), Some(STATE_SUPREME));

        let m = derive("calctapp", Some("SA"), "California Court of Appeal", None);
        assert_eq!(m.court_level.as_deref(), Some(STATE_APPELLATE));
    }

    #[test]
    fn west_virginia_is_not_virginia() {
        let m = derive("wva", Some("S"), "Supreme Court of Appeals of West Virginia", None);
        assert_eq!(m.jurisdiction.as_deref(), Some("WV"));
    }

    #[test]
    fn city_names_do_not_leak_a_state() {
        assert!(!contains_word("indianapolis municipal court", "indiana"));
        assert!(contains_word("court of appeals of indiana", "indiana"));
    }

    #[test]
    fn military_and_special_courts_are_federal_and_labelled() {
        let m = derive(
            "acca",
            Some("MA"),
            "Army Court of Criminal Appeals",
            Some("A.C.C.A."),
        );
        assert_eq!(m.jurisdiction.as_deref(), Some("US"));
        assert_eq!(m.method, "military_appellate");

        let m = derive("tax", Some("FS"), "United States Tax Court", None);
        assert_eq!(m.jurisdiction.as_deref(), Some("US"));
        assert_eq!(m.method, "federal_special");

        let m = derive(
            "bap9",
            Some("FBP"),
            "Bankruptcy Appellate Panel for the Ninth Circuit",
            None,
        );
        assert_eq!(m.method, "federal_bankruptcy_appellate");
    }

    #[test]
    fn a_state_court_is_not_swept_into_the_federal_branch() {
        // "cal" starts with "ca" but is not a numbered circuit.
        let m = derive("cal", Some("S"), "Supreme Court of California", None);
        assert_eq!(m.jurisdiction.as_deref(), Some("CA"));
        assert_eq!(m.court_level.as_deref(), Some(STATE_SUPREME));
    }

    #[test]
    fn territorial_courts_resolve_to_their_territory() {
        let cases = [
            ("guam", "TS", "Supreme Court of Guam", "GU"),
            ("prsupreme", "TS", "Supreme Court of Puerto Rico", "PR"),
            (
                "virginislands",
                "TS",
                "Supreme Court of The Virgin Islands",
                "VI",
            ),
            ("amsamoa", "TS", "High Court of American Samoa", "AS"),
            (
                "nmariana",
                "TS",
                "Supreme Court of The Commonwealth of The Northern Mariana Islands",
                "MP",
            ),
        ];
        for (id, class, name, expected) in cases {
            let m = derive(id, Some(class), name, None);
            assert_eq!(m.jurisdiction.as_deref(), Some(expected), "{id}");
            assert_eq!(m.court_level.as_deref(), Some(STATE_SUPREME), "{id}");
        }
    }

    #[test]
    fn tribal_courts_are_recorded_but_never_filed_under_state_law() {
        for (id, class, name) in [
            ("navajo", "TRS", "Navajo Nation Supreme Court"),
            ("hopiappct", "TRA", "Hopi Appellate Court"),
            (
                "echerokeect",
                "TRT",
                "Eastern Band of Cherokee Indians Tribal Court",
            ),
        ] {
            let m = derive(id, Some(class), name, None);
            assert_eq!(m.method, "tribal_sovereign", "{id}");
            assert!(!m.is_resolved(), "{id}");
            assert_eq!(m.court_level, None, "{id}");
        }
    }

    #[test]
    fn unmappable_courts_say_so() {
        let m = derive("mysterytribunal", Some("ST"), "Tribunal of Nowhere", None);
        assert!(!m.is_resolved());
        assert_eq!(m.method, "unmapped");
    }
}
