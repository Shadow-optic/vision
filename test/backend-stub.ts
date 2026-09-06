/**
 * Stands in for the Rust `vi-api` service during tests.
 *
 * Wired in as Miniflare's `outboundService`, so every request the Worker makes
 * to `VI_API_ORIGIN` lands here and no test can accidentally reach the network.
 */
import {
	ACTOR_ID,
	ENGINES,
	HELD_ACTOR_ID,
	INGEST_SOURCES,
	LEDGER,
	PIPELINE_STATUS,
	SCORE,
	SEARCH,
	SEARCH_FULL_CORPUS,
	TRACKER,
	TRANSPARENCY_SNAPSHOTS,
	WALL,
	WALL_PROFILE,
} from "./fixtures.ts";

const JSON_HEADERS = { "content-type": "application/json; charset=utf-8" };

function json(value: unknown, status = 200): Response {
	return new Response(JSON.stringify(value), { status, headers: JSON_HEADERS });
}

export function handleBackendRequest(request: Request): Response {
	const url = new URL(request.url);
	const path = url.pathname;

	if (request.method !== "GET") {
		return json({ error: "method_not_allowed" }, 405);
	}

	switch (path) {
		case "/health":
			return new Response("ok");
		case "/ready":
			return new Response("ready");
		case "/engines":
			return json(ENGINES);
		case "/ledger/verify":
			return json(LEDGER);
		case "/transparency/snapshots":
			return json(TRANSPARENCY_SNAPSHOTS);
		case "/reckoning/wall":
			return json(WALL);
		case "/reckoning/tracker":
			return json(TRACKER);
		case `/reckoning/wall/${ACTOR_ID}`:
			return json(WALL_PROFILE);
		case `/reckoning/wall/${HELD_ACTOR_ID}`:
			// A counsel hold is a 404 upstream, indistinguishable from "no record".
			return json({ error: "not_found" }, 404);
		case `/reckoning/actors/${ACTOR_ID}/score`:
			return json(SCORE);
		case "/cases/search": {
			const q = url.searchParams.get("q");
			if (q === "brady") return json(SEARCH);
			if (q === "complete") return json(SEARCH_FULL_CORPUS);
			// No hit, but the corpus is still mostly extracts.
			return json({
				results: [],
				corpus: { opinions: 89, full_text: 1, partial_text: 88, median_chars: 456 },
			});
		}
		case "/constitution/search":
			// Echoes the forwarded query so tests can assert parameter scrubbing.
			return json({ query: url.search, results: [] });
		case "/ingest/status":
			return json({ error: "boom" }, 500);
		case "/ingest/sources":
			return json(INGEST_SOURCES);
		case "/pipeline/status":
			return json(PIPELINE_STATUS);
		default:
			return json({ error: "not_found", path }, 404);
	}
}
