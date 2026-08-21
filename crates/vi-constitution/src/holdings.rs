//! Curated criminal-procedure interpretation snapshot. Incomplete by design;
//! original blackletter restatements + public citations only.
#![forbid(unsafe_code)]

use serde::Serialize;

use crate::clauses::CLAUSES;
use crate::jurisdictions::{ForumKind, JURISDICTIONS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CourtKind {
    Scotus,
    Circuit,
    State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    Controlling,
    CircuitBinding,
    Persuasive,
    Split,
    Overruled,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Holding {
    pub id: &'static str,
    pub citation: &'static str,
    pub year: i32,
    pub court_kind: CourtKind,
    pub court_id: &'static str,
    pub authority: Authority,
    pub rule_statement: &'static str,
    pub superseded_by: Option<&'static str>,
    pub clause_ids: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Split {
    pub id: &'static str,
    pub clause_id: &'static str,
    pub question: &'static str,
    pub side_a_circuits: &'static [&'static str],
    pub side_a_view: &'static str,
    pub side_b_circuits: &'static [&'static str],
    pub side_b_view: &'static str,
    pub notes: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Independent,
    Lockstep,
    Unspecified,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct StateCharter {
    pub code: &'static str,
    pub search: &'static str,
    pub due_process: &'static str,
    pub counsel: &'static str,
    pub confrontation: &'static str,
    pub cruel: &'static str,
    pub equal: &'static str,
    pub search_relation: Relation,
    pub more_protective_search: Option<bool>,
    pub notes: &'static str,
}

pub const HOLDINGS: &[Holding] = &[
    Holding {
        id: "weeks",
        citation: "Weeks v. United States, 232 U.S. 383 (1914)",
        year: 1914,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Evidence obtained in violation of the Fourth Amendment is excluded in federal prosecutions.",
        superseded_by: None,
        clause_ids: &["amend.04.search_seizure"],
    },
    Holding {
        id: "wolf",
        citation: "Wolf v. Colorado, 338 U.S. 25 (1949)",
        year: 1949,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Overruled,
        rule_statement: "Fourth Amendment privacy applies to the states, but the exclusionary rule does not (overruled in part by Mapp).",
        superseded_by: Some("mapp"),
        clause_ids: &["amend.04.search_seizure"],
    },
    Holding {
        id: "mapp",
        citation: "Mapp v. Ohio, 367 U.S. 643 (1961)",
        year: 1961,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Fourth Amendment exclusionary rule applies to the states through the Fourteenth Amendment.",
        superseded_by: None,
        clause_ids: &["amend.04.search_seizure", "amend.14.due_process"],
    },
    Holding {
        id: "katz",
        citation: "Katz v. United States, 389 U.S. 347 (1967)",
        year: 1967,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Fourth Amendment protects people, not places; a reasonable expectation of privacy is the touchstone.",
        superseded_by: None,
        clause_ids: &["amend.04.search_seizure"],
    },
    Holding {
        id: "terry",
        citation: "Terry v. Ohio, 392 U.S. 1 (1968)",
        year: 1968,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "A brief stop and frisk is reasonable on specific, articulable suspicion of crime and of present danger.",
        superseded_by: None,
        clause_ids: &["amend.04.search_seizure"],
    },
    Holding {
        id: "carpenter",
        citation: "Carpenter v. United States, 585 U.S. 296 (2018)",
        year: 2018,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Long-term cell-site location records are a search; a warrant is generally required.",
        superseded_by: None,
        clause_ids: &["amend.04.search_seizure"],
    },
    Holding {
        id: "hurtado",
        citation: "Hurtado v. California, 110 U.S. 516 (1884)",
        year: 1884,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Fifth Amendment grand-jury requirement is not incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.05.grand_jury"],
    },
    Holding {
        id: "malloy",
        citation: "Malloy v. Hogan, 378 U.S. 1 (1964)",
        year: 1964,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The privilege against self-incrimination is incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.05.self_incrimination"],
    },
    Holding {
        id: "miranda",
        citation: "Miranda v. Arizona, 384 U.S. 436 (1966)",
        year: 1966,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Custodial interrogation requires warnings and a voluntary waiver before statements may be used in the prosecution's case-in-chief.",
        superseded_by: None,
        clause_ids: &["amend.05.self_incrimination"],
    },
    Holding {
        id: "benton",
        citation: "Benton v. Maryland, 395 U.S. 784 (1969)",
        year: 1969,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Double Jeopardy Clause is incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.05.double_jeopardy"],
    },
    Holding {
        id: "gideon",
        citation: "Gideon v. Wainwright, 372 U.S. 335 (1963)",
        year: 1963,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Sixth Amendment right to counsel in felony prosecutions is incorporated; appointed counsel is required for indigents.",
        superseded_by: None,
        clause_ids: &["amend.06.counsel", "amend.14.due_process"],
    },
    Holding {
        id: "strickland",
        citation: "Strickland v. Washington, 466 U.S. 668 (1984)",
        year: 1984,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Ineffective assistance requires deficient performance and a reasonable probability of a different result.",
        superseded_by: None,
        clause_ids: &["amend.06.counsel"],
    },
    Holding {
        id: "pointer",
        citation: "Pointer v. Texas, 380 U.S. 400 (1965)",
        year: 1965,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Confrontation Clause is incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.06.confrontation"],
    },
    Holding {
        id: "crawford",
        citation: "Crawford v. Washington, 541 U.S. 36 (2004)",
        year: 2004,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Testimonial hearsay is inadmissible unless the witness is unavailable and the defendant had a prior opportunity to cross-examine.",
        superseded_by: None,
        clause_ids: &["amend.06.confrontation"],
    },
    Holding {
        id: "duncan",
        citation: "Duncan v. Louisiana, 391 U.S. 145 (1968)",
        year: 1968,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Sixth Amendment jury-trial right in serious criminal cases is incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.06.jury"],
    },
    Holding {
        id: "boykin",
        citation: "Boykin v. Alabama, 395 U.S. 238 (1969)",
        year: 1969,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "A guilty plea must be knowing and voluntary; the record must show a waiver of the privilege against self-incrimination, jury trial, and confrontation.",
        superseded_by: None,
        clause_ids: &["amend.06.jury", "amend.06.confrontation", "amend.05.self_incrimination", "amend.14.due_process"],
    },
    Holding {
        id: "bordenkircher",
        citation: "Bordenkircher v. Hayes, 434 U.S. 357 (1978)",
        year: 1978,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "A prosecutor may threaten additional charges during plea bargaining if the additional charge is supported by probable cause; that does not authorize a systemic practice that functionally punishes the choice to stand trial.",
        superseded_by: None,
        clause_ids: &["amend.06.jury", "amend.14.due_process"],
    },
    Holding {
        id: "brady",
        citation: "Brady v. Maryland, 373 U.S. 83 (1963)",
        year: 1963,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Suppression of material evidence favorable to the accused violates due process, irrespective of good or bad faith.",
        superseded_by: None,
        clause_ids: &["amend.14.due_process", "amend.05.due_process"],
    },
    Holding {
        id: "giglio",
        citation: "Giglio v. United States, 405 U.S. 150 (1972)",
        year: 1972,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Impeachment evidence concerning cooperating witnesses, including deals and benefits, is Brady material.",
        superseded_by: None,
        clause_ids: &["amend.14.due_process"],
    },
    Holding {
        id: "kyles",
        citation: "Kyles v. Whitley, 514 U.S. 419 (1995)",
        year: 1995,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The prosecutor is charged with knowledge of evidence known to police investigators acting on the case.",
        superseded_by: None,
        clause_ids: &["amend.14.due_process"],
    },
    Holding {
        id: "ruiz",
        citation: "United States v. Ruiz, 536 U.S. 622 (2002)",
        year: 2002,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Constitution does not require the government to disclose impeachment information before a guilty plea. Whether material exculpatory evidence must be disclosed before a plea remains an open question in several circuits.",
        superseded_by: None,
        clause_ids: &["amend.14.due_process"],
    },
    Holding {
        id: "batson",
        citation: "Batson v. Kentucky, 476 U.S. 79 (1986)",
        year: 1986,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Equal Protection Clause forbids striking jurors on the basis of race; a three-step burden-shifting inquiry applies.",
        superseded_by: None,
        clause_ids: &["amend.14.equal_protection", "amend.06.jury"],
    },
    Holding {
        id: "apprendi",
        citation: "Apprendi v. New Jersey, 530 U.S. 466 (2000)",
        year: 2000,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "Any fact (other than a prior conviction) that increases the penalty beyond the statutory maximum must be submitted to a jury and proved beyond a reasonable doubt.",
        superseded_by: None,
        clause_ids: &["amend.06.jury"],
    },
    Holding {
        id: "robinson",
        citation: "Robinson v. California, 370 U.S. 660 (1962)",
        year: 1962,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Eighth Amendment prohibition on cruel and unusual punishments is incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.08.cruel_unusual"],
    },
    Holding {
        id: "timbs",
        citation: "Timbs v. Indiana, 586 U.S. 146 (2019)",
        year: 2019,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Excessive Fines Clause is incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.08.excessive_fines"],
    },
    Holding {
        id: "mcdonald",
        citation: "McDonald v. City of Chicago, 561 U.S. 742 (2010)",
        year: 2010,
        court_kind: CourtKind::Scotus,
        court_id: "SCOTUS",
        authority: Authority::Controlling,
        rule_statement: "The Second Amendment is incorporated against the states.",
        superseded_by: None,
        clause_ids: &["amend.02.bear_arms"],
    },
    Holding {
        id: "brisendine",
        citation: "People v. Brisendine, 13 Cal.3d 528 (1975)",
        year: 1975,
        court_kind: CourtKind::State,
        court_id: "CA",
        authority: Authority::Persuasive,
        rule_statement: "California has construed its search-and-seizure charter independently of the Fourth Amendment; later Truth-in-Evidence (art. I §28) generally limits exclusion in criminal cases to the federal floor.",
        superseded_by: None,
        clause_ids: &["amend.04.search_seizure"],
    },
    Holding {
        id: "gunwall",
        citation: "State v. Gunwall, 106 Wn.2d 54 (1986)",
        year: 1986,
        court_kind: CourtKind::State,
        court_id: "WA",
        authority: Authority::Persuasive,
        rule_statement: "Washington Constitution article I, section 7 is more protective than the Fourth Amendment; independent-state-grounds analysis applies.",
        superseded_by: None,
        clause_ids: &["amend.04.search_seizure"],
    },
];

pub const SPLITS: &[Split] = &[
    Split {
        id: "brady-pre-plea-exculpatory",
        clause_id: "amend.14.due_process",
        question: "Whether due process requires disclosure of material exculpatory (not merely impeachment) evidence before a guilty plea, given United States v. Ruiz.",
        side_a_circuits: &["CA1", "CA2", "CA9"],
        side_a_view: "Ruiz is limited to impeachment; material exculpatory evidence must still be disclosed before a plea.",
        side_b_circuits: &["CA5", "CA7"],
        side_b_view: "Ruiz is read more broadly; the Constitution does not clearly require pre-plea disclosure of exculpatory evidence.",
        notes: "Curated snapshot of a live circuit disagreement. Not a complete survey. Status is unsettled where SCOTUS has not decided the exculpatory-before-plea question.",
    },
    Split {
        id: "geofence-warrants",
        clause_id: "amend.04.search_seizure",
        question: "Whether geofence and reverse-keyword warrants satisfy Fourth Amendment particularity and probable cause.",
        side_a_circuits: &["CA4"],
        side_a_view: "Geofence warrants as commonly structured raise grave particularity concerns and may be unconstitutional.",
        side_b_circuits: &["CA5", "CA7"],
        side_b_view: "Some geofence warrants have been upheld when narrowly drawn and supported by probable cause.",
        notes: "Carpenter supplies the location-privacy floor; geofence particularity remains a circuit-split overlay.",
    },
];

/// Native charter analogs for every selectable state (and PR). DC/territories without a full analog fall through to the federal Constitution.
pub const STATE_CHARTERS: &[StateCharter] = &[
    StateCharter { code: "AL", search: "Ala. Const. art. I, § 5", due_process: "Ala. Const. art. I, § 6 / § 13", counsel: "Ala. Const. art. I, § 6", confrontation: "Ala. Const. art. I, § 6", cruel: "Ala. Const. art. I, § 15", equal: "Ala. Const. art. I, § 1", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "AK", search: "Alaska Const. art. I, § 14", due_process: "Alaska Const. art. I, § 7", counsel: "Alaska Const. art. I, § 11", confrontation: "Alaska Const. art. I, § 11", cruel: "Alaska Const. art. I, § 12", equal: "Alaska Const. art. I, § 1", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Alaska often interprets privacy and search protections above the federal floor." },
    StateCharter { code: "AZ", search: "Ariz. Const. art. II, § 8", due_process: "Ariz. Const. art. II, § 4", counsel: "Ariz. Const. art. II, § 24", confrontation: "Ariz. Const. art. II, § 24", cruel: "Ariz. Const. art. II, § 15", equal: "Ariz. Const. art. II, § 13", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "AR", search: "Ark. Const. art. 2, § 15", due_process: "Ark. Const. art. 2, § 8", counsel: "Ark. Const. art. 2, § 10", confrontation: "Ark. Const. art. 2, § 10", cruel: "Ark. Const. art. 2, § 9", equal: "Ark. Const. art. 2, § 3", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "CA", search: "Cal. Const. art. I, § 13", due_process: "Cal. Const. art. I, § 7", counsel: "Cal. Const. art. I, § 15", confrontation: "Cal. Const. art. I, § 15", cruel: "Cal. Const. art. I, § 17", equal: "Cal. Const. art. I, § 7(a)", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Independent state grounds exist, but Cal. Const. art. I, § 28 (Truth-in-Evidence) generally limits exclusion of relevant evidence in criminal cases to the federal floor." },
    StateCharter { code: "CO", search: "Colo. Const. art. II, § 7", due_process: "Colo. Const. art. II, § 25", counsel: "Colo. Const. art. II, § 16", confrontation: "Colo. Const. art. II, § 16", cruel: "Colo. Const. art. II, § 20", equal: "Colo. Const. art. II, § 29", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "CT", search: "Conn. Const. art. I, § 7", due_process: "Conn. Const. art. I, § 8", counsel: "Conn. Const. art. I, § 8", confrontation: "Conn. Const. art. I, § 8", cruel: "Conn. Const. art. I, § 8", equal: "Conn. Const. art. I, § 20", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "DE", search: "Del. Const. art. I, § 6", due_process: "Del. Const. art. I, § 7", counsel: "Del. Const. art. I, § 7", confrontation: "Del. Const. art. I, § 7", cruel: "Del. Const. art. I, § 11", equal: "Del. Const. art. I, § 7", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "FL", search: "Fla. Const. art. I, § 12", due_process: "Fla. Const. art. I, § 9", counsel: "Fla. Const. art. I, § 16", confrontation: "Fla. Const. art. I, § 16", cruel: "Fla. Const. art. I, § 17", equal: "Fla. Const. art. I, § 2", search_relation: Relation::Lockstep, more_protective_search: Some(false), notes: "Article I, § 12 is construed in conformity with the Fourth Amendment as interpreted by the U.S. Supreme Court." },
    StateCharter { code: "GA", search: "Ga. Const. art. I, § I, para. XIII", due_process: "Ga. Const. art. I, § I, para. I", counsel: "Ga. Const. art. I, § I, para. XIV", confrontation: "Ga. Const. art. I, § I, para. XIV", cruel: "Ga. Const. art. I, § I, para. XVII", equal: "Ga. Const. art. I, § I, para. II", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "HI", search: "Haw. Const. art. I, § 7", due_process: "Haw. Const. art. I, § 5", counsel: "Haw. Const. art. I, § 14", confrontation: "Haw. Const. art. I, § 14", cruel: "Haw. Const. art. I, § 12", equal: "Haw. Const. art. I, § 5", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Hawaii privacy jurisprudence is frequently more protective than the Fourth Amendment." },
    StateCharter { code: "ID", search: "Idaho Const. art. I, § 17", due_process: "Idaho Const. art. I, § 13", counsel: "Idaho Const. art. I, § 13", confrontation: "Idaho Const. art. I, § 13", cruel: "Idaho Const. art. I, § 6", equal: "Idaho Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "IL", search: "Ill. Const. art. I, § 6", due_process: "Ill. Const. art. I, § 2", counsel: "Ill. Const. art. I, § 8", confrontation: "Ill. Const. art. I, § 8", cruel: "Ill. Const. art. I, § 11", equal: "Ill. Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "IN", search: "Ind. Const. art. 1, § 11", due_process: "Ind. Const. art. 1, § 12", counsel: "Ind. Const. art. 1, § 13", confrontation: "Ind. Const. art. 1, § 13", cruel: "Ind. Const. art. 1, § 16", equal: "Ind. Const. art. 1, § 23", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "IA", search: "Iowa Const. art. I, § 8", due_process: "Iowa Const. art. I, § 9", counsel: "Iowa Const. art. I, § 10", confrontation: "Iowa Const. art. I, § 10", cruel: "Iowa Const. art. I, § 17", equal: "Iowa Const. art. I, § 6", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "KS", search: "Kan. Const. Bill of Rights § 15", due_process: "Kan. Const. Bill of Rights § 18", counsel: "Kan. Const. Bill of Rights § 10", confrontation: "Kan. Const. Bill of Rights § 10", cruel: "Kan. Const. Bill of Rights § 9", equal: "Kan. Const. Bill of Rights § 1", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "KY", search: "Ky. Const. § 10", due_process: "Ky. Const. § 2", counsel: "Ky. Const. § 11", confrontation: "Ky. Const. § 11", cruel: "Ky. Const. § 17", equal: "Ky. Const. § 3", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "LA", search: "La. Const. art. I, § 5", due_process: "La. Const. art. I, § 2", counsel: "La. Const. art. I, § 13", confrontation: "La. Const. art. I, § 16", cruel: "La. Const. art. I, § 20", equal: "La. Const. art. I, § 3", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "ME", search: "Me. Const. art. I, § 5", due_process: "Me. Const. art. I, § 6-A", counsel: "Me. Const. art. I, § 6", confrontation: "Me. Const. art. I, § 6", cruel: "Me. Const. art. I, § 9", equal: "Me. Const. art. I, § 6-A", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "MD", search: "Md. Decl. of Rights art. 26", due_process: "Md. Decl. of Rights art. 24", counsel: "Md. Decl. of Rights art. 21", confrontation: "Md. Decl. of Rights art. 21", cruel: "Md. Decl. of Rights art. 16 / art. 25", equal: "Md. Decl. of Rights art. 24", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "MA", search: "Mass. Decl. of Rights art. XIV", due_process: "Mass. Decl. of Rights art. XII", counsel: "Mass. Decl. of Rights art. XII", confrontation: "Mass. Decl. of Rights art. XII", cruel: "Mass. Decl. of Rights art. XXVI", equal: "Mass. Decl. of Rights art. I", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Massachusetts often grants independent protection under art. XIV." },
    StateCharter { code: "MI", search: "Mich. Const. art. I, § 11", due_process: "Mich. Const. art. I, § 17", counsel: "Mich. Const. art. I, § 20", confrontation: "Mich. Const. art. I, § 20", cruel: "Mich. Const. art. I, § 16", equal: "Mich. Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "MN", search: "Minn. Const. art. I, § 10", due_process: "Minn. Const. art. I, § 7", counsel: "Minn. Const. art. I, § 6", confrontation: "Minn. Const. art. I, § 6", cruel: "Minn. Const. art. I, § 5", equal: "Minn. Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "MS", search: "Miss. Const. art. 3, § 23", due_process: "Miss. Const. art. 3, § 14", counsel: "Miss. Const. art. 3, § 26", confrontation: "Miss. Const. art. 3, § 26", cruel: "Miss. Const. art. 3, § 28", equal: "Miss. Const. art. 3, § 14", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "MO", search: "Mo. Const. art. I, § 15", due_process: "Mo. Const. art. I, § 10", counsel: "Mo. Const. art. I, § 18(a)", confrontation: "Mo. Const. art. I, § 18(a)", cruel: "Mo. Const. art. I, § 21", equal: "Mo. Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "MT", search: "Mont. Const. art. II, § 11", due_process: "Mont. Const. art. II, § 17", counsel: "Mont. Const. art. II, § 24", confrontation: "Mont. Const. art. II, § 24", cruel: "Mont. Const. art. II, § 22", equal: "Mont. Const. art. II, § 4", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Montana's explicit privacy clause is frequently more protective than the Fourth Amendment." },
    StateCharter { code: "NE", search: "Neb. Const. art. I, § 7", due_process: "Neb. Const. art. I, § 3", counsel: "Neb. Const. art. I, § 11", confrontation: "Neb. Const. art. I, § 11", cruel: "Neb. Const. art. I, § 9", equal: "Neb. Const. art. I, § 3", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "NV", search: "Nev. Const. art. 1, § 18", due_process: "Nev. Const. art. 1, § 8", counsel: "Nev. Const. art. 1, § 8", confrontation: "Nev. Const. art. 1, § 8", cruel: "Nev. Const. art. 1, § 6", equal: "Nev. Const. art. 1, § 1", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "NH", search: "N.H. Const. pt. 1, art. 19", due_process: "N.H. Const. pt. 1, art. 15", counsel: "N.H. Const. pt. 1, art. 15", confrontation: "N.H. Const. pt. 1, art. 15", cruel: "N.H. Const. pt. 1, art. 33", equal: "N.H. Const. pt. 1, art. 1", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "NJ", search: "N.J. Const. art. I, ¶ 7", due_process: "N.J. Const. art. I, ¶ 1", counsel: "N.J. Const. art. I, ¶ 10", confrontation: "N.J. Const. art. I, ¶ 10", cruel: "N.J. Const. art. I, ¶ 12", equal: "N.J. Const. art. I, ¶ 5", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "New Jersey frequently interprets art. I, ¶ 7 more protectively than the Fourth Amendment." },
    StateCharter { code: "NM", search: "N.M. Const. art. II, § 10", due_process: "N.M. Const. art. II, § 18", counsel: "N.M. Const. art. II, § 14", confrontation: "N.M. Const. art. II, § 14", cruel: "N.M. Const. art. II, § 13", equal: "N.M. Const. art. II, § 18", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "NY", search: "N.Y. Const. art. I, § 12", due_process: "N.Y. Const. art. I, § 6", counsel: "N.Y. Const. art. I, § 6", confrontation: "N.Y. Const. art. I, § 6", cruel: "N.Y. Const. art. I, § 5", equal: "N.Y. Const. art. I, § 11", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "New York has a substantial independent-state-grounds tradition under art. I, § 12." },
    StateCharter { code: "NC", search: "N.C. Const. art. I, § 20", due_process: "N.C. Const. art. I, § 19", counsel: "N.C. Const. art. I, § 23", confrontation: "N.C. Const. art. I, § 23", cruel: "N.C. Const. art. I, § 27", equal: "N.C. Const. art. I, § 19", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "ND", search: "N.D. Const. art. I, § 8", due_process: "N.D. Const. art. I, § 12", counsel: "N.D. Const. art. I, § 12", confrontation: "N.D. Const. art. I, § 13", cruel: "N.D. Const. art. I, § 11", equal: "N.D. Const. art. I, § 21", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "OH", search: "Ohio Const. art. I, § 14", due_process: "Ohio Const. art. I, § 16", counsel: "Ohio Const. art. I, § 10", confrontation: "Ohio Const. art. I, § 10", cruel: "Ohio Const. art. I, § 9", equal: "Ohio Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "OK", search: "Okla. Const. art. II, § 30", due_process: "Okla. Const. art. II, § 7", counsel: "Okla. Const. art. II, § 20", confrontation: "Okla. Const. art. II, § 20", cruel: "Okla. Const. art. II, § 9", equal: "Okla. Const. art. II, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "OR", search: "Or. Const. art. I, § 9", due_process: "Or. Const. art. I, § 10", counsel: "Or. Const. art. I, § 11", confrontation: "Or. Const. art. I, § 11", cruel: "Or. Const. art. I, § 16", equal: "Or. Const. art. I, § 20", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Oregon independently interprets article I, section 9." },
    StateCharter { code: "PA", search: "Pa. Const. art. I, § 8", due_process: "Pa. Const. art. I, § 9", counsel: "Pa. Const. art. I, § 9", confrontation: "Pa. Const. art. I, § 9", cruel: "Pa. Const. art. I, § 13", equal: "Pa. Const. art. I, § 26", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "RI", search: "R.I. Const. art. I, § 6", due_process: "R.I. Const. art. I, § 2", counsel: "R.I. Const. art. I, § 10", confrontation: "R.I. Const. art. I, § 10", cruel: "R.I. Const. art. I, § 8", equal: "R.I. Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "SC", search: "S.C. Const. art. I, § 10", due_process: "S.C. Const. art. I, § 3", counsel: "S.C. Const. art. I, § 14", confrontation: "S.C. Const. art. I, § 14", cruel: "S.C. Const. art. I, § 15", equal: "S.C. Const. art. I, § 3", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "SD", search: "S.D. Const. art. VI, § 11", due_process: "S.D. Const. art. VI, § 2", counsel: "S.D. Const. art. VI, § 7", confrontation: "S.D. Const. art. VI, § 7", cruel: "S.D. Const. art. VI, § 23", equal: "S.D. Const. art. VI, § 18", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "TN", search: "Tenn. Const. art. I, § 7", due_process: "Tenn. Const. art. I, § 8", counsel: "Tenn. Const. art. I, § 9", confrontation: "Tenn. Const. art. I, § 9", cruel: "Tenn. Const. art. I, § 16", equal: "Tenn. Const. art. I, § 8", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "TX", search: "Tex. Const. art. I, § 9", due_process: "Tex. Const. art. I, § 19", counsel: "Tex. Const. art. I, § 10", confrontation: "Tex. Const. art. I, § 10", cruel: "Tex. Const. art. I, § 13", equal: "Tex. Const. art. I, § 3", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "UT", search: "Utah Const. art. I, § 14", due_process: "Utah Const. art. I, § 7", counsel: "Utah Const. art. I, § 12", confrontation: "Utah Const. art. I, § 12", cruel: "Utah Const. art. I, § 9", equal: "Utah Const. art. I, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "VT", search: "Vt. Const. ch. I, art. 11", due_process: "Vt. Const. ch. I, art. 4", counsel: "Vt. Const. ch. I, art. 10", confrontation: "Vt. Const. ch. I, art. 10", cruel: "Vt. Const. ch. I, art. 10", equal: "Vt. Const. ch. I, art. 1", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Vermont article 11 is often read more protectively than the Fourth Amendment." },
    StateCharter { code: "VA", search: "Va. Const. art. I, § 10", due_process: "Va. Const. art. I, § 11", counsel: "Va. Const. art. I, § 8", confrontation: "Va. Const. art. I, § 8", cruel: "Va. Const. art. I, § 9", equal: "Va. Const. art. I, § 11", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "WA", search: "Wash. Const. art. I, § 7", due_process: "Wash. Const. art. I, § 3", counsel: "Wash. Const. art. I, § 22", confrontation: "Wash. Const. art. I, § 22", cruel: "Wash. Const. art. I, § 14", equal: "Wash. Const. art. I, § 12", search_relation: Relation::Independent, more_protective_search: Some(true), notes: "Article I, section 7 (privacy) is more protective than the Fourth Amendment (State v. Gunwall)." },
    StateCharter { code: "WV", search: "W. Va. Const. art. III, § 6", due_process: "W. Va. Const. art. III, § 10", counsel: "W. Va. Const. art. III, § 14", confrontation: "W. Va. Const. art. III, § 14", cruel: "W. Va. Const. art. III, § 5", equal: "W. Va. Const. art. III, § 10", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "WI", search: "Wis. Const. art. I, § 11", due_process: "Wis. Const. art. I, § 8", counsel: "Wis. Const. art. I, § 7", confrontation: "Wis. Const. art. I, § 7", cruel: "Wis. Const. art. I, § 6", equal: "Wis. Const. art. I, § 1", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "WY", search: "Wyo. Const. art. 1, § 4", due_process: "Wyo. Const. art. 1, § 6", counsel: "Wyo. Const. art. 1, § 10", confrontation: "Wyo. Const. art. 1, § 10", cruel: "Wyo. Const. art. 1, § 14", equal: "Wyo. Const. art. 1, § 2", search_relation: Relation::Unspecified, more_protective_search: None, notes: "State charter may provide additional protection; holdings snapshot incomplete." },
    StateCharter { code: "PR", search: "P.R. Const. art. II, § 10", due_process: "P.R. Const. art. II, § 7", counsel: "P.R. Const. art. II, § 11", confrontation: "P.R. Const. art. II, § 11", cruel: "P.R. Const. art. II, § 12", equal: "P.R. Const. art. II, § 1", search_relation: Relation::Unspecified, more_protective_search: None, notes: "Puerto Rico's bill of rights applies alongside the federal Constitution." },
];

#[derive(Debug, Clone, Copy, Serialize)]
pub struct StateAnalog {
    pub code: &'static str,
    pub clause_id: &'static str,
    pub state_citation: &'static str,
    pub relation: Relation,
    pub more_protective: Option<bool>,
    pub notes: &'static str,
}

pub fn state_charter(code: &str) -> Option<&'static StateCharter> {
    STATE_CHARTERS
        .iter()
        .find(|s| s.code.eq_ignore_ascii_case(code))
}

pub fn analogs_for(code: &str) -> Vec<StateAnalog> {
    let Some(c) = state_charter(code) else {
        return Vec::new();
    };
    vec![
        analog(
            c,
            "amend.04.search_seizure",
            c.search,
            c.search_relation,
            c.more_protective_search,
        ),
        analog(
            c,
            "amend.05.due_process",
            c.due_process,
            Relation::Unspecified,
            None,
        ),
        analog(
            c,
            "amend.14.due_process",
            c.due_process,
            Relation::Unspecified,
            None,
        ),
        analog(
            c,
            "amend.06.counsel",
            c.counsel,
            Relation::Unspecified,
            None,
        ),
        analog(
            c,
            "amend.06.confrontation",
            c.confrontation,
            Relation::Unspecified,
            None,
        ),
        analog(
            c,
            "amend.08.cruel_unusual",
            c.cruel,
            Relation::Unspecified,
            None,
        ),
        analog(
            c,
            "amend.14.equal_protection",
            c.equal,
            Relation::Unspecified,
            None,
        ),
    ]
}

fn analog(
    c: &StateCharter,
    clause_id: &'static str,
    citation: &'static str,
    relation: Relation,
    more_protective: Option<bool>,
) -> StateAnalog {
    StateAnalog {
        code: c.code,
        clause_id,
        state_citation: citation,
        relation,
        more_protective,
        notes: c.notes,
    }
}

pub fn holding(id: &str) -> Option<&'static Holding> {
    HOLDINGS.iter().find(|h| h.id == id)
}

pub fn holdings_for_clause(clause_id: &str) -> Vec<&'static Holding> {
    HOLDINGS
        .iter()
        .filter(|h| h.clause_ids.contains(&clause_id))
        .collect()
}

pub fn all_state_analogs() -> Vec<StateAnalog> {
    STATE_CHARTERS
        .iter()
        .flat_map(|c| analogs_for(c.code))
        .collect()
}

pub fn validate_snapshot() -> Result<(), String> {
    for h in HOLDINGS {
        for cid in h.clause_ids {
            if CLAUSES.iter().all(|c| c.id != *cid) {
                return Err(format!("holding {} unknown clause {cid}", h.id));
            }
        }
        if let Some(s) = h.superseded_by {
            if holding(s).is_none() {
                return Err(format!("holding {} superseded_by missing {s}", h.id));
            }
        }
    }
    for s in SPLITS {
        if CLAUSES.iter().all(|c| c.id != s.clause_id) {
            return Err(format!("split {} unknown clause", s.id));
        }
    }
    let state_codes: Vec<_> = JURISDICTIONS
        .iter()
        .filter(|j| j.kind == ForumKind::State)
        .map(|j| j.code)
        .collect();
    if state_codes.len() != 50 {
        return Err("expected 50 states".into());
    }
    for code in state_codes {
        if state_charter(code).is_none() {
            return Err(format!("missing state charter analog for {code}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_internally_consistent() {
        validate_snapshot().unwrap();
        assert!(holding("mapp").is_some());
        assert!(holding("brady").is_some());
        assert_eq!(STATE_CHARTERS.len(), 51); // 50 states + PR
        assert_eq!(analogs_for("CA").len(), 7);
        assert_eq!(analogs_for("WY").len(), 7);
        assert_eq!(
            state_charter("FL").unwrap().search_relation,
            Relation::Lockstep
        );
        assert_eq!(
            state_charter("WA").unwrap().more_protective_search,
            Some(true)
        );
    }
}
