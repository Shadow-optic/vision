import { describe, expect, it } from "vitest";
import { escapeHtml, html, raw } from "../worker/view/html";
import { Router, isUuid } from "../worker/router";
import { ALLOWED, WITHHELD, buildUpstreamPath, matchAllowed } from "../worker/proxy";
import { config } from "../worker/env";
import { STATUTES, IMMUNITY, authorizesLife } from "../worker/data/catalog";

describe("html escaping", () => {
	it("escapes every interpolated value", () => {
		const name = '<script>alert("x")</script>';
		expect(html`<p>${name}</p>`.value).toBe(
			"<p>&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;</p>",
		);
	});

	it("escapes attribute-breaking quotes and ampersands", () => {
		expect(escapeHtml(`" onload='x' & <b>`)).toBe(
			"&quot; onload=&#39;x&#39; &amp; &lt;b&gt;",
		);
	});

	it("keeps nested fragments unescaped but escapes their inputs", () => {
		const inner = html`<em>${"a & b"}</em>`;
		expect(html`<p>${inner}</p>`.value).toBe("<p><em>a &amp; b</em></p>");
	});

	it("renders arrays and drops nullish values", () => {
		expect(html`${[1, "<", null, undefined, false]}`.value).toBe("1&lt;");
	});

	it("only trusts explicitly raw markup", () => {
		expect(html`${raw("<hr>")}`.value).toBe("<hr>");
	});
});

describe("router", () => {
	const router = new Router<string>()
		.get("/", () => new Response("root"))
		.get("/wall", () => new Response("wall"))
		.get("/wall/:id", () => new Response("profile"));

	it("matches static and parameterized paths", () => {
		expect(router.lookup("GET", "/")).not.toBeNull();
		const hit = router.lookup("GET", "/wall/abc");
		expect(hit && "match" in hit && hit.match.params.id).toBe("abc");
	});

	it("treats HEAD as GET", () => {
		expect(router.lookup("HEAD", "/wall")).not.toBeNull();
	});

	it("reports a method mismatch separately from a missing path", () => {
		expect(router.lookup("POST", "/wall")).toEqual({ handler: null });
		expect(router.lookup("GET", "/nope")).toBeNull();
	});

	it("validates uuids", () => {
		expect(isUuid("11111111-1111-1111-1111-111111111111")).toBe(true);
		expect(isUuid("../../etc/passwd")).toBe(false);
	});
});

describe("public api allowlist", () => {
	it("publishes the register but never pending flags", () => {
		expect(matchAllowed("/reckoning/wall")).not.toBeNull();
		expect(matchAllowed("/reckoning/wall/11111111-1111-1111-1111-111111111111")).not.toBeNull();
		expect(matchAllowed("/flags")).toBeNull();
	});

	it("withholds unpublished individuals, scores, and work product", () => {
		expect(matchAllowed("/reckoning/actors")).toBeNull();
		expect(matchAllowed("/reckoning/actors/abc/score")).toBeNull();
		expect(matchAllowed("/reckoning/packages/abc")).toBeNull();
		expect(matchAllowed("/lasm/package/abc")).toBeNull();
		expect(matchAllowed("/brady/lead-report/abc")).toBeNull();
		expect(matchAllowed("/prosecutors/abc/stats")).toBeNull();
		expect(matchAllowed("/trial-penalty/judges")).toBeNull();
	});

	it("documents every exclusion it enforces", () => {
		expect(WITHHELD.length).toBeGreaterThan(4);
		expect(WITHHELD.every((w) => w.reason.length > 20)).toBe(true);
	});

	it("rejects traversal and oversized path parameters", () => {
		expect(matchAllowed("/cases/..%2f..%2fetc")).toBeNull();
		expect(matchAllowed("/cases/../secrets")).toBeNull();
		expect(matchAllowed(`/cases/${"a".repeat(200)}`)).toBeNull();
	});

	it("forwards only declared query parameters", () => {
		const route = matchAllowed("/cases/search");
		expect(route).not.toBeNull();
		const url = new URL("https://site.test/api/cases/search?q=brady&limit=5&evil=1");
		expect(buildUpstreamPath(route!, url, "/cases/search")).toBe(
			"/cases/search?q=brady&limit=5",
		);
	});

	it("drops the query string entirely for routes that take none", () => {
		const route = matchAllowed("/reckoning/wall");
		const url = new URL("https://site.test/api/reckoning/wall?include_held=true");
		expect(buildUpstreamPath(route!, url, "/reckoning/wall")).toBe("/reckoning/wall");
	});

	it("forwards the office filter the plea-sentence correlation requires", () => {
		const route = matchAllowed("/stats/plea-sentence");
		expect(route).not.toBeNull();
		const url = new URL(
			"https://site.test/api/stats/plea-sentence?office=Demo%20County%20DA&jurisdiction=CA&evil=1",
		);
		expect(buildUpstreamPath(route!, url, "/stats/plea-sentence")).toBe(
			"/stats/plea-sentence?office=Demo+County+DA&jurisdiction=CA",
		);
	});

	it("allows the transparency proof log reads", () => {
		expect(matchAllowed("/transparency/snapshots")).not.toBeNull();
		expect(matchAllowed("/transparency/snapshots/latest")).not.toBeNull();
		expect(matchAllowed("/transparency/proof/wall_entries/abc-123")).not.toBeNull();
	});

	it("withholds pending machine-derived resonance, drift, and capture artifacts", () => {
		// These are pending leads about identifiable cases and named officials;
		// the doctrine forbids publishing pending automated output.
		expect(matchAllowed("/resonance/cases")).toBeNull();
		expect(matchAllowed("/resonance/case/abc")).toBeNull();
		expect(matchAllowed("/drift/changepoints")).toBeNull();
		expect(matchAllowed("/capture/outliers")).toBeNull();
		for (const path of ["/resonance/cases", "/drift/changepoints", "/capture/outliers"]) {
			expect(
				WITHHELD.some((w) => w.path.includes(path)),
				`${path} must be a documented exclusion on /api`,
			).toBe(true);
		}
	});

	it("caches every allowlisted read except liveness probes", () => {
		for (const route of ALLOWED) {
			if (route.pattern === "/health" || route.pattern === "/ready") continue;
			expect(route.ttl).toBeGreaterThan(0);
		}
	});
});

describe("configuration", () => {
	it("requires https for a backend origin", () => {
		expect(config({ VI_API_ORIGIN: "https://api.test" }).apiOrigin).toBe("https://api.test");
		expect(config({ VI_API_ORIGIN: "http://api.test" }).apiOrigin).toBeNull();
		expect(config({ VI_API_ORIGIN: "not a url" }).apiOrigin).toBeNull();
		expect(config({}).apiOrigin).toBeNull();
	});

	it("allows plain http on localhost for local development", () => {
		expect(config({ VI_API_ORIGIN: "http://localhost:8080" }).apiOrigin).toBe(
			"http://localhost:8080",
		);
	});

	it("strips a trailing slash so path joins stay clean", () => {
		expect(config({ VI_API_ORIGIN: "https://api.test/" }).apiOrigin).toBe("https://api.test");
	});
});

describe("statute catalog generated from the rust crate", () => {
	it("carries the color-of-law titles with their elements", () => {
		const cites = STATUTES.map((s) => s.citation);
		expect(cites).toContain("18 U.S.C. § 242");
		expect(cites).toContain("18 U.S.C. § 241");
		expect(cites).toContain("42 U.S.C. § 1983");
		expect(STATUTES.every((s) => s.elements.length > 0)).toBe(true);
	});

	it("flags exactly the titles whose maximum reaches life", () => {
		const life = STATUTES.filter(authorizesLife).map((s) => s.citation);
		expect(life).toContain("18 U.S.C. § 242");
		expect(life).toContain("18 U.S.C. § 241");
		expect(life).toContain("18 U.S.C. § 1512");
		expect(life).not.toContain("42 U.S.C. § 1983");
	});

	it("keeps the recognized limits on every immunity doctrine", () => {
		expect(IMMUNITY.length).toBeGreaterThanOrEqual(3);
		expect(IMMUNITY.every((n) => n.recognized_limits.length > 20)).toBe(true);
	});
});
