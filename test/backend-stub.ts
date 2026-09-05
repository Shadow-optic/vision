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
	LEDGER,
	SCORE,
	SEARCH,
	TRACKER,
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
		case "/cases/search":
			return json(url.searchParams.get("q") === "brady" ? SEARCH : { results: [] });
		case "/constitution/search":
			// Echoes the forwarded query so tests can assert parameter scrubbing.
			return json({ query: url.search, results: [] });
		case "/ingest/status":
			return json({ error: "boom" }, 500);
		default:
			return json({ error: "not_found", path }, 404);
	}
}
