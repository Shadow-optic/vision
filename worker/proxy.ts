/**
 * Read-only JSON mirror of the backend at `/api/*`.
 *
 * The allowlist below is a publication guardrail, not a convenience. Anything
 * that would expose a pending flag, an unpublished individual, a victim-side
 * lead, or attorney work product is absent by design, and every write route on
 * the backend — publication holds, package generation, rule runs, ingestion —
 * is unreachable from this domain. `GET` only; the proxy never forwards a body.
 */
import type { SiteConfig } from "./env";
import { jsonResponse, textResponse } from "./http";
import { requestUpstream } from "./upstream";

export interface AllowedRoute {
	/** Path pattern with `:name` segments. */
	pattern: string;
	/** Query parameters forwarded upstream; everything else is dropped. */
	params?: string[];
	/** Edge cache lifetime in seconds. */
	ttl: number;
	/** What the route returns, shown on the /api page. */
	summary: string;
}

export const ALLOWED: AllowedRoute[] = [
	{ pattern: "/health", ttl: 0, summary: "Backend liveness." },
	{ pattern: "/ready", ttl: 0, summary: "Backend readiness, including the database." },
	{ pattern: "/engines", ttl: 30, summary: "Every engine with its live row count." },
	{ pattern: "/ledger/verify", ttl: 30, summary: "Recompute and verify the whole hash chain." },
	{
		pattern: "/reckoning/wall",
		ttl: 60,
		summary: "Published register: officials with counsel-substantiated public-record findings.",
	},
	{
		pattern: "/reckoning/wall/:id",
		ttl: 60,
		summary: "One published register card. 404 when no card exists or counsel has placed a hold.",
	},
	{
		pattern: "/reckoning/tracker",
		ttl: 60,
		summary: "Attorney-reviewed referrals and packages for published officials.",
	},
	{
		pattern: "/reckoning/statutes",
		ttl: 3600,
		summary: "Statute catalog: elements, statutory maxima, research notes.",
	},
	{
		pattern: "/reckoning/immunity",
		ttl: 3600,
		summary: "Immunity doctrines and the limits courts recognize.",
	},
	{
		pattern: "/cases/search",
		params: ["q", "limit"],
		ttl: 120,
		summary:
			"Search over ingested public opinions. Reports how much of the stored corpus is complete text, because most feed records are extracts.",
	},
	{ pattern: "/cases/:id", ttl: 300, summary: "Case context for one ingested public case." },
	{ pattern: "/constitution", ttl: 3600, summary: "Constitutional corpus catalog." },
	{ pattern: "/constitution/options", ttl: 3600, summary: "Dropdown options for all 50 states." },
	{
		pattern: "/constitution/jurisdictions",
		params: ["kind"],
		ttl: 3600,
		summary: "Jurisdictions and their federal circuits.",
	},
	{
		pattern: "/constitution/provisions",
		params: ["kind"],
		ttl: 3600,
		summary: "Articles I–VII and Amendments 1–27.",
	},
	{ pattern: "/constitution/provisions/:id", ttl: 3600, summary: "One provision with its clauses." },
	{ pattern: "/constitution/clauses", ttl: 3600, summary: "All clauses in the corpus." },
	{
		pattern: "/constitution/search",
		params: ["q", "limit"],
		ttl: 300,
		summary: "Full-text search of the constitutional corpus.",
	},
	{
		pattern: "/tactics",
		params: ["category"],
		ttl: 300,
		summary: "Documented courtroom tactics catalog.",
	},
	{ pattern: "/tactics/:id", ttl: 300, summary: "One tactic." },
	{ pattern: "/tactics/:id/stats", ttl: 300, summary: "Occurrence rates from public records." },
	{
		pattern: "/trial-penalty/offices",
		params: ["office", "jurisdiction"],
		ttl: 300,
		summary: "Plea-versus-trial sentence distribution for an office.",
	},
	{
		pattern: "/trial-penalty/heatmap",
		params: ["resolution", "jurisdiction"],
		ttl: 300,
		summary: "Geospatial trial-penalty heatmap.",
	},
	{
		pattern: "/trial-penalty/disparity",
		params: ["group_a", "group_b", "charge_category", "threshold"],
		ttl: 300,
		summary: "Disparity odds ratio between two groups.",
	},
	{
		pattern: "/atlas/offices/fingerprint",
		params: ["office", "jurisdiction"],
		ttl: 300,
		summary: "Pattern-and-practice fingerprint for an office.",
	},
	{
		pattern: "/atlas/offices/monell-report",
		params: ["office", "jurisdiction"],
		ttl: 300,
		summary: "Municipal-liability report for an office.",
	},
	{ pattern: "/stats/plea-sentence", ttl: 300, summary: "Plea-rate to sentence-length correlation." },
	{ pattern: "/geo/cells/:cell", ttl: 300, summary: "Aggregates for one H3 cell." },
	{
		pattern: "/geo/kring/:cell",
		params: ["k"],
		ttl: 300,
		summary: "Aggregates across a k-ring of H3 cells.",
	},
	{ pattern: "/ingest/status", ttl: 60, summary: "Ingest cursors and last-run status." },
	{
		pattern: "/ingest/sources",
		ttl: 60,
		summary:
			"The public feeds this deployment reads, each one's last success or failure, and the categories ingestion refuses to store. Provenance about the platform; it names no individual.",
	},
];

/** Documented, deliberate exclusions — rendered on the /api page. */
export const WITHHELD: { path: string; reason: string }[] = [
	{
		path: "/flags",
		reason:
			"Pending automated flags. Publishing an unreviewed flag against a named official is exactly what the doctrine forbids.",
	},
	{
		path: "/reckoning/actors",
		reason:
			"Includes officials with no substantiated finding. Only the published register is public.",
	},
	{
		path: "/reckoning/actors/:id/score",
		reason:
			"An abuse score outside the context of a published card invites misreading as a verdict.",
	},
	{
		path: "/reckoning/packages/:id",
		reason: "Attorney work product prepared for a licensed attorney's signature.",
	},
	{
		path: "/brady/*, /lasm/package/:case_id",
		reason: "Investigative leads and defense-side case packages. A gap is a lead, not a finding.",
	},
	{
		path: "/prosecutors/:id/stats, /trial-penalty/judges",
		reason:
			"Individual-level statistics without a counsel-substantiated finding attached. Office-level aggregates are published instead.",
	},
	{
		path: "/pipeline/status",
		reason:
			"Reports pending flags per identifiable case, which is the same disclosure /flags is withheld for. The aggregate totals appear on /sources instead.",
	},
	{
		path: "/pipeline/unresolved-officials",
		reason:
			"Quotes the raw judge field of an identifiable case while the platform has declined to say which individuals it names. The open count appears on /sources instead.",
	},
	{
		path: "/ingest/cycle, /ingest/run, /ingest/place-courts",
		reason:
			"Starting ingestion, placing courts, and walking records through the engines are counsel operations. The public site publishes feed state at /ingest/sources; it never starts a poll.",
	},
	{
		path: "All POST routes",
		reason:
			"Publication and holds, package generation, rule runs, entity resolution, and ingestion are counsel operations. They exist only on the backend.",
	},
];

function segments(path: string): string[] {
	return path.split("/").filter((s) => s !== "");
}

export function matchAllowed(path: string): AllowedRoute | null {
	const actual = segments(path);
	for (const route of ALLOWED) {
		const pattern = segments(route.pattern);
		if (pattern.length !== actual.length) continue;
		let ok = true;
		for (let i = 0; i < pattern.length; i++) {
			const p = pattern[i] as string;
			if (p.startsWith(":")) {
				const value = actual[i] as string;
				// Path parameters are opaque identifiers; keep them boring.
				if (!/^[A-Za-z0-9._-]{1,80}$/.test(value)) {
					ok = false;
					break;
				}
				continue;
			}
			if (p !== (actual[i] as string)) {
				ok = false;
				break;
			}
		}
		if (ok) return route;
	}
	return null;
}

export function buildUpstreamPath(route: AllowedRoute, url: URL, backendPath: string): string {
	const allowed = route.params ?? [];
	const forwarded = new URLSearchParams();
	for (const name of allowed) {
		const value = url.searchParams.get(name);
		if (value !== null && value !== "") forwarded.set(name, value.slice(0, 300));
	}
	const qs = forwarded.toString();
	return qs === "" ? backendPath : `${backendPath}?${qs}`;
}

/** Serves `/api/<backend path>`. */
export async function proxy(cfg: SiteConfig, url: URL, backendPath: string): Promise<Response> {
	const route = matchAllowed(backendPath);
	if (!route) {
		return jsonResponse(
			{
				error: "not_allowed",
				message:
					"This path is not part of the public read-only mirror. See /api for the allowlist and the reasons for each exclusion.",
				path: backendPath,
			},
			{ status: 404 },
		);
	}

	if (!cfg.apiOrigin) {
		return jsonResponse(
			{
				error: "backend_not_connected",
				message:
					"VI_API_ORIGIN is not set on this Worker, so no live data can be served. Doctrine, statute, and immunity content is served from the Worker itself.",
				path: backendPath,
			},
			{ status: 503 },
		);
	}

	const upstreamPath = buildUpstreamPath(route, url, backendPath);
	let res: Response;
	try {
		const r = await requestUpstream(cfg, upstreamPath, 8000);
		if (!r) throw new Error("no upstream");
		res = r;
	} catch (err) {
		return jsonResponse(
			{
				error: "backend_unavailable",
				message: err instanceof Error ? err.message : "upstream request failed",
				path: backendPath,
			},
			{ status: 502 },
		);
	}

	const contentType = res.headers.get("content-type") ?? "application/json; charset=utf-8";
	const body = await res.text();
	if (!contentType.includes("json")) {
		return textResponse(body, {
			status: res.status,
			contentType,
			cacheSeconds: res.ok ? route.ttl : 0,
		});
	}

	return new Response(body, {
		status: res.status,
		headers: {
			"content-type": "application/json; charset=utf-8",
			"cache-control": res.ok && route.ttl > 0 ? `public, max-age=${route.ttl}` : "no-store",
		},
	});
}
