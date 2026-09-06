/** Response shapes served by the Rust `vi-api` service. */

export interface PublicRecord {
	finding_type: string;
	citation: string | null;
	summary: string | null;
	finding_date: string | null;
	source_url: string | null;
}

export interface WallEntry {
	actor_id: string;
	role: string;
	display_name: string;
	office: string | null;
	jurisdiction: string;
	bar_number: string | null;
	badge_number: string | null;
	substantiated_findings: number;
	public_records: PublicRecord[];
	status: string;
}

export interface WallResponse {
	name: string;
	also_known_as?: string;
	gate: string;
	charges: boolean;
	entries: WallEntry[];
}

export interface WallProfileResponse {
	name: string;
	charges: boolean;
	entry: WallEntry;
}

export interface AbuseScore {
	actor_id: string;
	score: number;
	substantiated_findings: number;
	substantiated_flags: number;
	corroboration_sources: number;
	recent_5yr: number;
	formula: string;
}

export interface EngineRow {
	name: string;
	crate: string;
	rows: number;
	routes: string[];
}

export interface EnginesResponse {
	backend: string;
	database: string;
	engines: EngineRow[];
}

export interface TrackerRow {
	package_id: string;
	actor_id: string;
	display_name: string;
	action_kind: string;
	status: string;
}

export interface TrackerResponse {
	packages: TrackerRow[];
}

export interface LedgerVerifyResponse {
	entries: number;
	ok: boolean;
	first_bad_seq: number | null;
	tip_hash: string | null;
}

export interface CaseHit {
	case_id: string;
	docket_number: string | null;
	jurisdiction: string | null;
	outcome: string | null;
	citation: string | null;
	date_issued: string | null;
	rank: number | null;
	/** `full`, or `snippet`/`summary` for a partial extract from a feed. */
	text_completeness?: string | null;
	source_url?: string | null;
	matched_on?: string | null;
}

/** How much of the stored corpus is complete opinion text. */
export interface SearchCorpus {
	opinions: number;
	full_text: number;
	partial_text: number;
	median_chars: number;
}

export interface SearchResponse {
	results: CaseHit[];
	corpus?: SearchCorpus;
	caveat?: string;
}

export interface IngestCursorStatus {
	feed_kind?: string | null;
	label?: string | null;
	last_ok_at?: string | null;
	last_polled_at?: string | null;
	last_error?: string | null;
	consecutive_failures?: number;
	new_cases?: number;
	new_opinions?: number;
	total_skipped?: number;
}

export interface IngestSource {
	source: string;
	label?: string | null;
	configured: boolean;
	status?: IngestCursorStatus | null;
}

export interface IngestSourcesResponse {
	sources: IngestSource[];
	/** Categories never stored, whatever a feed publishes. */
	exclusions?: string[];
	note?: string;
}

export interface PipelineStatusResponse {
	cases?: { total: number; awaiting_pipeline: number };
	stages?: string[];
	totals?: {
		runs?: number;
		screens?: number;
		flags_fired?: number;
		evidence_gaps?: number;
		actors_linked?: number;
	};
	unresolved_officials?: {
		open_ambiguous?: number;
		open_collective?: number;
		closed?: number;
	};
	note?: string;
}
