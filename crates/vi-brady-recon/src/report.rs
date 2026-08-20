use crate::reconcile::ReconReport;
use handlebars::Handlebars;

const TEMPLATE: &str = r#"# Brady Reconciliation Lead Report — Attorney Work Product
**Case:** {{case_id}} | **Run ID:** {{run_id}}

This report identifies *potential* gaps between evidence referenced in the public record and evidence disclosed in discovery. Each gap is a research lead, not a proven Brady violation.

- Expected evidence items: {{expected_count}}
- Disclosed evidence items: {{disclosed_count}}
- Potential gaps: {{gap_count}}

{{#each gaps}}
### {{item_type}}
- **Description:** {{description}}
- **Source reference:** {{source_reference}}
- **Suggested FOIA / discovery request:** {{suggested_foia}}
{{/each}}

## Next Steps
1. Issue the suggested discovery or FOIA requests.
2. Compare responses against the expected inventory.
3. If material remains undisclosed and is favorable to the defense, evaluate a Brady motion or post-conviction claim.
"#;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RenderError(String);

impl From<handlebars::TemplateError> for RenderError {
    fn from(e: handlebars::TemplateError) -> Self {
        Self(e.to_string())
    }
}
impl From<handlebars::RenderError> for RenderError {
    fn from(e: handlebars::RenderError) -> Self {
        Self(e.to_string())
    }
}

pub fn render(report: &ReconReport) -> Result<String, RenderError> {
    let mut h = Handlebars::new();
    h.register_template_string("brady", TEMPLATE)?;
    Ok(h.render("brady", report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reconcile::{Gap, ReconReport};
    use uuid::Uuid;

    #[test]
    fn labels_as_leads_not_findings() {
        let md = render(&ReconReport {
            run_id: Uuid::nil(),
            case_id: Uuid::nil(),
            expected_count: 1,
            disclosed_count: 0,
            gap_count: 1,
            gaps: vec![Gap {
                expected_id: Uuid::nil(),
                item_type: "bodycam".into(),
                description: "Body-worn camera footage".into(),
                source_reference: "Demo".into(),
                suggested_foia: "Request all bodycam records.".into(),
            }],
        })
        .unwrap();
        assert!(md.contains("research lead"));
        assert!(md.contains("not a proven Brady violation"));
        assert!(md.contains("Attorney Work Product"));
    }
}
