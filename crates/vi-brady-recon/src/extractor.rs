use regex::Regex;
use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::OnceLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExtractedItem {
    pub item_type: &'static str,
    pub description: String,
    pub source_reference: String,
}

struct Pattern {
    item_type: &'static str,
    regex: Regex,
    desc_template: &'static str,
}

fn patterns() -> &'static Vec<Pattern> {
    static P: OnceLock<Vec<Pattern>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            Pattern {
                item_type: "witness_interview",
                regex: Regex::new(
                    r"(?i)(?:interview(?:ed)?|statement(?:s)?)\s+(?:with\s+)?([A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+){0,2})",
                )
                .expect("witness regex"),
                desc_template: "Interview/statement of {}",
            },
            Pattern {
                item_type: "bodycam",
                regex: Regex::new(r"(?i)body[- ]?worn\s+(?:camera|video|footage)").expect("bodycam regex"),
                desc_template: "Body-worn camera footage",
            },
            Pattern {
                item_type: "lab_report",
                regex: Regex::new(r"(?i)(?:lab(?:oratory)?|forensic)\s+report(?:s)?").expect("lab regex"),
                desc_template: "Laboratory/forensic report",
            },
            Pattern {
                item_type: "chain_of_custody",
                regex: Regex::new(r"(?i)chain\s+of\s+custody").expect("coc regex"),
                desc_template: "Chain-of-custody documentation",
            },
            Pattern {
                item_type: "informant_benefit",
                regex: Regex::new(
                    r"(?i)(?:informant|cooperat(?:ing|or)|cooperator)\s+(?:received|was\s+promised|benefits?|leniency|immunity|deal)",
                )
                .expect("informant regex"),
                desc_template: "Informant/cooperator benefit disclosures",
            },
            Pattern {
                item_type: "911_call",
                regex: Regex::new(r"(?i)911\s+(?:call|recording|dispatch)").expect("911 regex"),
                desc_template: "911 call/recording",
            },
            Pattern {
                item_type: "forensic_worksheet",
                regex: Regex::new(r"(?i)forensic\s+worksheet|analyst\s+notes|bench\s+notes")
                    .expect("worksheet regex"),
                desc_template: "Forensic analyst worksheets/notes",
            },
        ]
    })
}

pub fn extract_from_text(text: &str, source_reference: &str) -> Vec<ExtractedItem> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for pat in patterns() {
        for cap in pat.regex.captures_iter(text) {
            let desc = if let Some(m) = cap.get(1) {
                pat.desc_template.replace("{}", m.as_str().trim())
            } else {
                pat.desc_template.to_string()
            };
            let key = format!("{}|{}", pat.item_type, desc.to_lowercase());
            if seen.insert(key) {
                out.push(ExtractedItem {
                    item_type: pat.item_type,
                    description: desc,
                    source_reference: source_reference.to_string(),
                });
            }
        }
    }
    out
}

pub async fn derive_expected_for_case(
    pool: &PgPool,
    case_id: Uuid,
) -> Result<Vec<ExtractedItem>, sqlx::Error> {
    let opinions: Vec<(Option<String>, String)> =
        sqlx::query_as("SELECT citation, full_text FROM court_opinions WHERE case_id=$1")
            .bind(case_id)
            .fetch_all(pool)
            .await?;

    let mut all = Vec::new();
    for (citation, text) in opinions {
        let src = citation.unwrap_or_else(|| "unspecified opinion".into());
        all.extend(extract_from_text(&text, &src));
    }

    for item in &all {
        sqlx::query(
            "INSERT INTO expected_evidence_items
             (case_id, item_type, description, source_reference)
             VALUES ($1,$2,$3,$4)
             ON CONFLICT DO NOTHING",
        )
        .bind(case_id)
        .bind(item.item_type)
        .bind(&item.description)
        .bind(&item.source_reference)
        .execute(pool)
        .await?;
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO_OPINION: &str = r#"The defendant moved to suppress evidence obtained during the traffic stop.
  The motion was denied. Body-worn camera footage was referenced by the arresting
  officer but the chain of custody documentation was not produced. A laboratory
  report and forensic worksheet were cited in testimony. The defendant rejected
  a plea offer of twelve months and was convicted at trial and sentenced to
  thirty-six months. Counsel noted that a 911 call recording was never disclosed."#;

    #[test]
    fn extracts_seed_opinion_items() {
        let items = extract_from_text(DEMO_OPINION, "Demo v. Demo (2024)");
        let types: Vec<_> = items.iter().map(|i| i.item_type).collect();
        assert!(types.contains(&"bodycam"));
        assert!(types.contains(&"chain_of_custody"));
        assert!(types.contains(&"lab_report"));
        assert!(types.contains(&"forensic_worksheet"));
        assert!(types.contains(&"911_call"));
        assert!(items
            .iter()
            .all(|i| i.source_reference == "Demo v. Demo (2024)"));
    }

    #[test]
    fn dedupes_same_type() {
        let items = extract_from_text(
            "chain of custody. The chain of custody was incomplete.",
            "x",
        );
        assert_eq!(
            items
                .iter()
                .filter(|i| i.item_type == "chain_of_custody")
                .count(),
            1
        );
    }
}
