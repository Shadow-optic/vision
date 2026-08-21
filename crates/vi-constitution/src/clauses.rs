//! Clause graph over the Constitution. Clauses are the units the resolver binds.
#![forbid(unsafe_code)]

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Incorporation {
    /// Applies against the states through the Fourteenth Amendment.
    Incorporated,
    /// Supreme Court has held it does not bind the states.
    NotIncorporated,
    /// Incorporation status is unsettled or only partial.
    Unsettled,
    /// Provision is not a Bill of Rights clause (e.g. Art. I, Amend. XIV itself).
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Clause {
    pub id: &'static str,
    pub provision_id: &'static str,
    pub label: &'static str,
    pub bill_of_rights: bool,
    pub criminal_procedure: bool,
    pub incorporation: Incorporation,
}

pub const CLAUSES: &[Clause] = &[
    Clause {
        id: "art.01.sec.09.habeas",
        provision_id: "art.01.sec.09",
        label: "Privilege of the writ of habeas corpus",
        bill_of_rights: false,
        criminal_procedure: true,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "art.01.sec.09.attainder",
        provision_id: "art.01.sec.09",
        label: "Bill of attainder",
        bill_of_rights: false,
        criminal_procedure: true,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "art.01.sec.09.ex_post_facto",
        provision_id: "art.01.sec.09",
        label: "Ex post facto",
        bill_of_rights: false,
        criminal_procedure: true,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "art.03.sec.02.jury",
        provision_id: "art.03.sec.02",
        label: "Article III jury trial",
        bill_of_rights: false,
        criminal_procedure: true,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "art.06.supremacy",
        provision_id: "art.06",
        label: "Supremacy Clause",
        bill_of_rights: false,
        criminal_procedure: false,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "amend.01.religion",
        provision_id: "amend.01",
        label: "Establishment and free exercise",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.01.speech",
        provision_id: "amend.01",
        label: "Freedom of speech",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.01.press",
        provision_id: "amend.01",
        label: "Freedom of the press",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.01.assembly",
        provision_id: "amend.01",
        label: "Peaceable assembly and petition",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.02.bear_arms",
        provision_id: "amend.02",
        label: "Keep and bear arms",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.03.quartering",
        provision_id: "amend.03",
        label: "Quartering of soldiers",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::NotIncorporated,
    },
    Clause {
        id: "amend.04.search_seizure",
        provision_id: "amend.04",
        label: "Unreasonable searches and seizures; warrants",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.05.grand_jury",
        provision_id: "amend.05",
        label: "Grand-jury indictment",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::NotIncorporated,
    },
    Clause {
        id: "amend.05.double_jeopardy",
        provision_id: "amend.05",
        label: "Double jeopardy",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.05.self_incrimination",
        provision_id: "amend.05",
        label: "Privilege against self-incrimination",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.05.due_process",
        provision_id: "amend.05",
        label: "Fifth Amendment due process",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.05.takings",
        provision_id: "amend.05",
        label: "Takings / just compensation",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.06.speedy",
        provision_id: "amend.06",
        label: "Speedy trial",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.06.public_trial",
        provision_id: "amend.06",
        label: "Public trial",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.06.jury",
        provision_id: "amend.06",
        label: "Impartial jury",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.06.accusation",
        provision_id: "amend.06",
        label: "Notice of accusation",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.06.confrontation",
        provision_id: "amend.06",
        label: "Confrontation",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.06.compulsory_process",
        provision_id: "amend.06",
        label: "Compulsory process",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.06.counsel",
        provision_id: "amend.06",
        label: "Assistance of counsel",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.07.civil_jury",
        provision_id: "amend.07",
        label: "Civil jury trial",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::NotIncorporated,
    },
    Clause {
        id: "amend.08.excessive_bail",
        provision_id: "amend.08",
        label: "Excessive bail",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Unsettled,
    },
    Clause {
        id: "amend.08.excessive_fines",
        provision_id: "amend.08",
        label: "Excessive fines",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.08.cruel_unusual",
        provision_id: "amend.08",
        label: "Cruel and unusual punishments",
        bill_of_rights: true,
        criminal_procedure: true,
        incorporation: Incorporation::Incorporated,
    },
    Clause {
        id: "amend.09.unenumerated",
        provision_id: "amend.09",
        label: "Unenumerated rights retained by the people",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "amend.10.reserved",
        provision_id: "amend.10",
        label: "Powers reserved to the states and the people",
        bill_of_rights: true,
        criminal_procedure: false,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "amend.13.involuntary_servitude",
        provision_id: "amend.13",
        label: "Involuntary servitude",
        bill_of_rights: false,
        criminal_procedure: false,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "amend.14.due_process",
        provision_id: "amend.14.sec.01",
        label: "Fourteenth Amendment due process",
        bill_of_rights: false,
        criminal_procedure: true,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "amend.14.equal_protection",
        provision_id: "amend.14.sec.01",
        label: "Equal protection of the laws",
        bill_of_rights: false,
        criminal_procedure: true,
        incorporation: Incorporation::NotApplicable,
    },
    Clause {
        id: "amend.14.privileges_immunities",
        provision_id: "amend.14.sec.01",
        label: "Privileges or immunities",
        bill_of_rights: false,
        criminal_procedure: false,
        incorporation: Incorporation::NotApplicable,
    },
];

pub fn get(id: &str) -> Option<&'static Clause> {
    CLAUSES.iter().find(|c| c.id == id)
}

pub fn bill_of_rights() -> impl Iterator<Item = &'static Clause> {
    CLAUSES.iter().filter(|c| c.bill_of_rights)
}

pub fn criminal_procedure() -> impl Iterator<Item = &'static Clause> {
    CLAUSES.iter().filter(|c| c.criminal_procedure)
}

impl Incorporation {
    pub fn binds_states(self) -> bool {
        matches!(
            self,
            Incorporation::Incorporated | Incorporation::NotApplicable
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bill_of_rights_and_criminal_coverage() {
        let bor: Vec<_> = bill_of_rights().collect();
        assert!(bor.len() >= 10);
        assert!(get("amend.04.search_seizure").is_some());
        assert!(get("amend.06.counsel").is_some());
        assert!(get("amend.14.due_process").is_some());
        assert_eq!(
            get("amend.05.grand_jury").unwrap().incorporation,
            Incorporation::NotIncorporated
        );
        assert_eq!(
            get("amend.07.civil_jury").unwrap().incorporation,
            Incorporation::NotIncorporated
        );
        assert!(criminal_procedure().count() >= 15);
    }
}
