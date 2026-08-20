use crate::{DisclosedItem, Error, ExpectedItem, EVENT_RECON};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    pub expected_id: Uuid,
    pub item_type: String,
    pub description: String,
    pub source_reference: String,
    pub suggested_foia: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconReport {
    pub run_id: Uuid,
    pub case_id: Uuid,
    pub expected_count: usize,
    pub disclosed_count: usize,
    pub gap_count: usize,
    pub gaps: Vec<Gap>,
}

pub fn compute_gaps(expected: &[ExpectedItem], disclosed: &[DisclosedItem]) -> Vec<Gap> {
    let mut gaps = Vec::new();
    for e in expected {
        let expected_norm = e.description.to_lowercase();
        let matched = disclosed.iter().any(|d| {
            d.item_type == e.item_type
                && (d.description.to_lowercase().contains(&expected_norm)
                    || expected_norm.contains(&d.description.to_lowercase()))
        });
        if !matched {
            let suggested_foia = format!(
                "Request all {} records described as '{}' referenced in {}.",
                e.item_type.replace('_', " "),
                e.description,
                e.source_reference
            );
            gaps.push(Gap {
                expected_id: e.item_id,
                item_type: e.item_type.clone(),
                description: e.description.clone(),
                source_reference: e.source_reference.clone(),
                suggested_foia,
            });
        }
    }
    gaps
}

pub async fn reconcile(
    pool: &PgPool,
    ledger: &Ledger,
    case_id: Uuid,
) -> Result<ReconReport, Error> {
    let expected: Vec<ExpectedItem> = sqlx::query_as(
        "SELECT item_id, case_id, item_type, description, source_reference
         FROM expected_evidence_items WHERE case_id=$1",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await?;

    let disclosed: Vec<DisclosedItem> = sqlx::query_as(
        "SELECT item_id, case_id, item_type, description, disclosed_date,
                disclosed_by, source_url, raw_metadata
         FROM disclosed_evidence_items WHERE case_id=$1",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await?;

    let gaps = compute_gaps(&expected, &disclosed);

    let run_id = Uuid::new_v4();
    let report = ReconReport {
        run_id,
        case_id,
        expected_count: expected.len(),
        disclosed_count: disclosed.len(),
        gap_count: gaps.len(),
        gaps,
    };

    sqlx::query(
        "INSERT INTO brady_recon_runs (run_id, case_id, gaps_found, report)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(run_id)
    .bind(case_id)
    .bind(report.gap_count as i32)
    .bind(json!(&report))
    .execute(pool)
    .await?;

    ledger
        .append(
            EVENT_RECON,
            &json!({
                "run_id": run_id,
                "case_id": case_id,
                "gaps_found": report.gap_count,
                "report_hash": vi_ledger::hash_payload(&json!(&report)),
            }),
        )
        .await?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn expected(desc: &str, ty: &str) -> ExpectedItem {
        ExpectedItem {
            item_id: Uuid::nil(),
            case_id: Uuid::nil(),
            item_type: ty.into(),
            description: desc.into(),
            source_reference: "Demo v. Demo".into(),
        }
    }

    fn disclosed(desc: &str, ty: &str) -> DisclosedItem {
        DisclosedItem {
            item_id: Uuid::nil(),
            case_id: Uuid::nil(),
            item_type: ty.into(),
            description: desc.into(),
            disclosed_date: None,
            disclosed_by: None,
            source_url: None,
            raw_metadata: json!({}),
        }
    }

    #[test]
    fn unmatched_is_a_gap() {
        let gaps = compute_gaps(&[expected("Body-worn camera footage", "bodycam")], &[]);
        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].suggested_foia.contains("bodycam"));
    }

    #[test]
    fn overlapping_descriptions_match() {
        let gaps = compute_gaps(
            &[expected("Body-worn camera footage", "bodycam")],
            &[disclosed("body-worn camera", "bodycam")],
        );
        assert!(gaps.is_empty());
    }

    #[test]
    fn type_mismatch_is_a_gap() {
        let gaps = compute_gaps(
            &[expected("Body-worn camera footage", "bodycam")],
            &[disclosed("Body-worn camera footage", "lab_report")],
        );
        assert_eq!(gaps.len(), 1);
    }
}
