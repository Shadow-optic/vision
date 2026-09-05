import { describe, expect, it } from "vitest";
import type { WALL } from "./fixtures";
import { get, normalize } from "./helpers";

function api(path: string, init: RequestInit = {}): Promise<Response> {
	return get(`/api${path}`, init);
}

describe("public JSON mirror", () => {
	it("serves the register with permissive CORS and an edge cache", async () => {
		const res = await api("/reckoning/wall");
		expect(res.status).toBe(200);
		expect(res.headers.get("access-control-allow-origin")).toBe("*");
		expect(res.headers.get("cache-control")).toContain("max-age=60");
		const body = (await res.json()) as typeof WALL;
		expect(body.charges).toBe(false);
		expect(body.entries).toHaveLength(1);
		expect(body.gate).toContain("licensed-counsel");
	});

	it("refuses paths outside the allowlist with a pointer to the policy", async () => {
		for (const path of [
			"/flags",
			"/reckoning/actors",
			"/reckoning/packages/33333333-3333-3333-3333-333333333333",
			"/prosecutors/11111111-1111-1111-1111-111111111111/stats",
			"/rules/run",
		]) {
			const res = await api(path);
			expect(res.status, path).toBe(404);
			const body = (await res.json()) as { error: string; message: string };
			expect(body.error).toBe("not_allowed");
			expect(body.message).toContain("/api");
		}
	});

	it("rejects writes even on allowlisted paths", async () => {
		const res = await api("/reckoning/wall", { method: "POST" });
		expect(res.status).toBe(405);
		const body = (await res.json()) as { error: string };
		expect(body.error).toBe("read_only");
	});

	it("forwards only declared query parameters", async () => {
		const res = await api("/constitution/search?q=due+process&limit=3&evil=1");
		expect(res.status).toBe(200);
		// The stub echoes the query string it actually received.
		const body = (await res.json()) as { query: string };
		expect(body.query).toBe("?q=due+process&limit=3");
	});

	it("passes an upstream failure through without caching it", async () => {
		const res = await api("/ingest/status");
		expect(res.status).toBe(500);
		expect(res.headers.get("cache-control")).toBe("no-store");
	});

	it("keeps the API documentation page reachable at /api", async () => {
		const res = await get("/api");
		expect(res.status).toBe(200);
		expect(res.headers.get("content-type")).toContain("text/html");
		const page = normalize(await res.text());
		expect(page).toContain("Allowlisted endpoints");
		expect(page).toContain("What is deliberately not here");
		expect(page).toContain("/api/reckoning/wall");
	});
});
