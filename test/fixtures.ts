/** Stub payloads shaped exactly like the Rust `vi-api` responses. */

export const ACTOR_ID = "11111111-1111-1111-1111-111111111111";
export const HELD_ACTOR_ID = "22222222-2222-2222-2222-222222222222";

/** The display name is hostile on purpose: publication must never become markup. */
export const HOSTILE_NAME = 'Dana <script>alert("xss")</script> Reyes';

export const WALL_ENTRY = {
	actor_id: ACTOR_ID,
	role: "prosecutor",
	display_name: HOSTILE_NAME,
	office: "Demo County District Attorney",
	jurisdiction: "CA",
	bar_number: "SBN-100200",
	badge_number: null,
	substantiated_findings: 2,
	public_records: [
		{
			finding_type: "brady",
			citation: "People v. Demo, 1 Cal.App.5th 1 (2019)",
			summary: "Exculpatory lab report withheld until after verdict.",
			finding_date: "2019-04-02",
			source_url: "https://courts.example.gov/opinions/1",
		},
		{
			finding_type: "witness_subornation",
			citation: "In re Demo, 2 Cal.5th 2 (2021)",
			summary: "Testimony procured after coaching contradicted by recording.",
			finding_date: "2021-08-15",
			source_url: null,
		},
	],
	status: "referred",
};

export const WALL = {
	name: "Public Accountability Register",
	also_known_as: "Wall of Injustice",
	gate: "licensed-counsel substantiation of public-record findings; pending flags never publish",
	charges: false,
	entries: [WALL_ENTRY],
};

export const WALL_PROFILE = {
	name: "Public Accountability Register",
	charges: false,
	entry: WALL_ENTRY,
};

export const SCORE = {
	actor_id: ACTOR_ID,
	score: 46,
	substantiated_findings: 2,
	substantiated_flags: 1,
	corroboration_sources: 2,
	recent_5yr: 1,
	formula:
		"min(100, 12*min(findings,4) + 8*min(flags,3) + 6*min(sources,3) + (10 if recent_5yr>0 else 0))",
};

export const TRACKER = {
	packages: [
		{
			package_id: "33333333-3333-3333-3333-333333333333",
			actor_id: ACTOR_ID,
			display_name: HOSTILE_NAME,
			action_kind: "criminal_referral",
			status: "referred",
		},
	],
};

export const ENGINES = {
	backend: "vi-api",
	database: "ready",
	engines: [
		{ name: "Root Ledger", crate: "vi-ledger", rows: 12, routes: ["/ledger/verify"] },
		{
			name: "Reckoning / individual accountability",
			crate: "vi-reckoning",
			rows: 4,
			routes: ["/reckoning/wall"],
		},
	],
};

export const LEDGER = {
	entries: 12,
	ok: true,
	first_bad_seq: null,
	tip_hash: "b3aabbccddeeff00112233445566778899aabbccddeeff001122334455667788",
};

export const SEARCH = {
	results: [
		{
			case_id: "44444444-4444-4444-4444-444444444444",
			docket_number: "CR-2019-0001",
			jurisdiction: "CA",
			outcome: "reversed",
			citation: "People v. Demo, 1 Cal.App.5th 1 (2019)",
			date_issued: "2019-04-02",
			rank: 0.42,
		},
	],
};
