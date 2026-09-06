/**
 * Public-read API served by the Worker when `VI_API_ORIGIN` is unset.
 *
 * This is how the live site populates without a separate Rust process: the
 * same JSON shapes `vi-api` publishes, filled from public CourtListener
 * feeds and from catalogs that already ship in the Worker. The Wall stays
 * empty until licensed counsel substantiates a finding on the backend —
 * an empty register is honest; inventing an official is not.
 *
 * Counsel writes (ingest start, publication, packages) are still withheld.
 */
import { ENGINES } from "./data/engines";
import { IMMUNITY, STATUTES } from "./data/catalog";

const CL = "https://www.courtlistener.com";
const USER_AGENT =
	"VisionInjustice/0.1 (public-records accountability research; +https://github.com/Shadow-optic/vision)";

const EXCLUSIONS = ["sealed", "juvenile", "expunged", "source-blocked", "empty text"];

const DEFAULT_FEEDS: { source: string; label: string; kind: string }[] = [
	{
		source: "courtlistener-courts",
		label: "CourtListener courts registry",
		kind: "registry",
	},
	{
		source: "courtlistener-search/brady-violation",
		label: "CourtListener search: “brady violation”",
		kind: "search",
	},
	{
		source: "courtlistener-search/prosecutorial-misconduct",
		label: "CourtListener search: “prosecutorial misconduct”",
		kind: "search",
	},
	{
		source: "courtlistener-search/fabricated-evidence",
		label: "CourtListener search: “fabricated evidence”",
		kind: "search",
	},
	{
		source: "courtlistener-feed/ca9",
		label: "CourtListener Atom feed: ca9",
		kind: "atom",
	},
	{
		source: "courtlistener-feed/scotus",
		label: "CourtListener Atom feed: scotus",
		kind: "atom",
	},
];

function json(value: unknown, status = 200): Response {
	return new Response(JSON.stringify(value), {
		status,
		headers: { "content-type": "application/json; charset=utf-8" },
	});
}

function text(body: string, status = 200): Response {
	return new Response(body, {
		status,
		headers: { "content-type": "text/plain; charset=utf-8" },
	});
}

/** Dispatch a public-read path the same way `vi-api` would. */
export async function edgeGet(pathWithQuery: string): Promise<Response> {
	const url = new URL(pathWithQuery, "https://edge.visioninjustice.internal");
	const path = url.pathname;

	switch (path) {
		case "/health":
			return text("ok");
		case "/ready":
			return text("ready");
		case "/engines":
			return json(engines());
		case "/ledger/verify":
			return json({
				entries: 0,
				ok: true,
				first_bad_seq: null,
				tip_hash: null,
			});
		case "/reckoning/wall":
			return json({
				name: "Public Accountability Register",
				also_known_as: "Wall of Injustice",
				gate: "licensed-counsel substantiation of public-record findings; pending flags never publish",
				charges: false,
				entries: [],
			});
		case "/reckoning/tracker":
			return json({ packages: [] });
		case "/reckoning/statutes":
			return json({ statutes: STATUTES });
		case "/reckoning/immunity":
			return json({ immunity: IMMUNITY });
		case "/cases/search":
			return searchCases(url.searchParams.get("q") ?? "", url.searchParams.get("limit"));
		case "/ingest/status":
			return json({ cursors: [] });
		case "/ingest/sources":
			return sources();
		case "/pipeline/status":
			return json({
				cases: { total: 0, awaiting_pipeline: 0 },
				stages: [
					"forum",
					"constitution_screen",
					"evidence_leads",
					"abuse_rules",
					"actor_links",
					"score",
				],
				totals: {
					runs: 0,
					screens: 0,
					flags_fired: 0,
					evidence_gaps: 0,
					actors_linked: 0,
				},
				unresolved_officials: {
					open_ambiguous: 0,
					open_collective: 0,
					closed: 0,
				},
				note: "The public edge does not run the counsel pipeline. Publication is a separate act by licensed counsel.",
			});
		case "/tactics":
			return json({ tactics: [] });
		case "/stats/plea-sentence":
			return json({ n: 0, note: "No stored office statistics on the public edge." });
		default:
			if (path.startsWith("/reckoning/wall/")) return json({ error: "not_found" }, 404);
			if (path.startsWith("/cases/")) return json({ error: "not_found" }, 404);
			if (path.startsWith("/tactics/")) return json({ error: "not_found" }, 404);
			if (path.startsWith("/constitution")) return constitution(path, url.searchParams);
			if (path.startsWith("/geo/")) return json({ error: "not_found" }, 404);
			if (path.startsWith("/atlas/")) return json({ error: "not_found" }, 404);
			if (path.startsWith("/trial-penalty/")) return json({ error: "not_found" }, 404);
			return json({ error: "not_found", path }, 404);
	}
}

function engines() {
	return {
		backend: "vision-edge",
		database: "edge-public-read",
		engines: ENGINES.map((e) => ({
			name: e.name,
			crate: e.crate,
			rows: 0,
			routes: e.routes,
		})),
	};
}

function sources(): Response {
	return json({
		sources: DEFAULT_FEEDS.map((f) => ({
			source: f.source,
			label: f.label,
			configured: true,
			status: {
				feed_kind: f.kind,
				label: f.label,
				last_ok_at: null,
				last_polled_at: null,
				last_error: null,
				last_pause: null,
				next_url: null,
				consecutive_failures: 0,
				new_cases: 0,
				new_opinions: 0,
				total_skipped: 0,
			},
		})),
		totals: {
			cases: 0,
			opinions: 0,
			partial_text_opinions: 0,
			courts_registered: 0,
		},
		exclusions: EXCLUSIONS,
		note: "The public edge reads CourtListener live for search. Stored ingest cursors and counsel artifacts live on vi-api. Ingestion concludes nothing about any individual.",
	});
}

function constitution(path: string, params: URLSearchParams): Response {
	if (path === "/constitution" || path === "/constitution/options") {
		return json({ source: "worker-catalog", note: "Full corpus is served by vi-api." });
	}
	if (path === "/constitution/search") {
		return json({ query: params.toString() ? `?${params}` : "", results: [] });
	}
	if (
		path === "/constitution/jurisdictions" ||
		path === "/constitution/provisions" ||
		path === "/constitution/clauses"
	) {
		return json({ results: [] });
	}
	return json({ error: "not_found", path }, 404);
}

/**
 * Live CourtListener opinion search. Results are snippets from the public
 * search API and are labelled as such — a caption page is not an opinion.
 */
export async function searchCases(rawQuery: string, rawLimit: string | null): Promise<Response> {
	const q = rawQuery.trim().slice(0, 200);
	const limit = Math.max(1, Math.min(50, Number(rawLimit) || 20));
	if (q === "") {
		return json({
			results: [],
			corpus: { opinions: 0, full_text: 0, partial_text: 0, median_chars: 0 },
			caveat: "Enter a query to search the public CourtListener corpus.",
		});
	}

	const url = `${CL}/api/rest/v4/search/?q=${encodeURIComponent(q)}&type=o&order_by=score+desc&page_size=${limit}`;
	let body: { results?: unknown[] };
	try {
		const res = await fetch(url, {
			method: "GET",
			headers: { accept: "application/json", "user-agent": USER_AGENT },
			signal: AbortSignal.timeout(8000),
		});
		if (!res.ok) {
			return json(
				{
					error: "upstream",
					message: `CourtListener search returned ${res.status}`,
				},
				502,
			);
		}
		body = (await res.json()) as { results?: unknown[] };
	} catch (err) {
		const message = err instanceof Error ? err.message : "CourtListener search failed";
		return json({ error: "upstream", message }, 502);
	}

	const mapped = (body.results ?? []).map(mapHit).filter((h) => h !== null);
	const results = mapped.map(({ _chars: _n, ...hit }) => hit);
	return json({
		results,
		corpus: {
			opinions: results.length,
			full_text: 0,
			partial_text: results.length,
			median_chars: medianChars(mapped.map((r) => r._chars)),
		},
		caveat:
			"These hits are search extracts from CourtListener, not stored complete opinions. A term absent here may still appear in the full opinion. An empty result is not evidence that no such case exists.",
	});
}

function medianChars(values: number[]): number {
	if (values.length === 0) return 0;
	const sorted = [...values].sort((a, b) => a - b);
	const mid = Math.floor(sorted.length / 2);
	const a = sorted[mid];
	const b = sorted[mid - 1];
	if (sorted.length % 2 === 1) return a ?? 0;
	return Math.round(((a ?? 0) + (b ?? 0)) / 2);
}

function mapHit(raw: unknown): (Record<string, unknown> & { _chars: number }) | null {
	if (!raw || typeof raw !== "object") return null;
	const r = raw as Record<string, unknown>;
	const docket =
		asString(r.docketNumber) ||
		(typeof r.docket_id === "number" ? `cl-docket:${r.docket_id}` : null);
	if (!docket) return null;
	const blob = `${docket} ${JSON.stringify(r)}`.toLowerCase();
	if (blob.includes("sealed") || blob.includes("juvenile") || blob.includes("expunged")) {
		return null;
	}
	const opinions = Array.isArray(r.opinions) ? r.opinions : [];
	const first = (opinions[0] && typeof opinions[0] === "object" ? opinions[0] : {}) as Record<
		string,
		unknown
	>;
	const snippet = asString(first.snippet) ?? "";
	const opinionId = typeof first.id === "number" ? first.id : null;
	const caseName = asString(r.caseName);
	const court = asString(r.court_id);
	const filed = asString(r.dateFiled);
	const abs = asString(r.absolute_url);
	return {
		case_id: opinionId !== null ? `cl-${opinionId}` : `cl-${docket.replace(/[^A-Za-z0-9._-]/g, "").slice(0, 40)}`,
		docket_number: docket,
		jurisdiction: court,
		outcome: null,
		citation: caseName ?? docket,
		date_issued: filed ? filed.slice(0, 10) : null,
		rank: typeof r.score === "number" ? r.score : null,
		text_completeness: "snippet",
		source_url: abs ? (abs.startsWith("http") ? abs : `${CL}${abs}`) : `${CL}/`,
		matched_on: snippet ? "snippet" : "docket",
		_chars: snippet.length,
	};
}

function asString(value: unknown): string | null {
	return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

export const EDGE_FEEDS = DEFAULT_FEEDS;
export const EDGE_EXCLUSIONS = EXCLUSIONS;
