import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { ENGINES } from "../worker/data/engines";
import { ACTOR_ID, HELD_ACTOR_ID, LEDGER } from "./fixtures";
import { SITE, body, get, normalize } from "./helpers";

describe("security posture", () => {
	it("locks down every HTML response", async () => {
		const res = await get("/");
		expect(res.status).toBe(200);
		const csp = res.headers.get("content-security-policy") ?? "";
		expect(csp).toContain("default-src 'none'");
		expect(csp).toContain("script-src 'self'");
		expect(csp).not.toContain("unsafe-inline");
		expect(res.headers.get("x-frame-options")).toBe("DENY");
		expect(res.headers.get("x-content-type-options")).toBe("nosniff");
		expect(res.headers.get("referrer-policy")).toBe("strict-origin-when-cross-origin");
		expect(res.headers.get("cross-origin-resource-policy")).toBe("same-origin");
		expect(res.headers.get("strict-transport-security")).toContain("max-age=31536000");
		expect(res.headers.get("x-request-id")).toBeTruthy();
	});

	it("refuses writes on the public site", async () => {
		const res = await get("/wall", { method: "POST" });
		expect(res.status).toBe(405);
		expect(res.headers.get("allow")).toBe("GET, HEAD");
	});

	it("serves no body for HEAD but keeps the headers", async () => {
		const res = await get("/", { method: "HEAD" });
		expect(res.status).toBe(200);
		expect(await res.text()).toBe("");
		expect(res.headers.get("content-type")).toContain("text/html");
	});
});

describe("landing page", () => {
	it("states the mission, the gate, and the live totals", async () => {
		const page = await body("/");
		expect(page).toContain("Individual Accountability Doctrine");
		expect(page).toContain("Wall of Injustice");
		expect(page).toContain("Officials published");
		expect(page).toContain("does not file charges");
		// live numbers from the stubbed backend: 1 published official, 12 ledger entries
		expect(page).toContain('<div class="n">1</div>');
		expect(page).toContain('<div class="n">12</div>');
		expect(page).toContain("chain verified");
	});
});

describe("wall of injustice", () => {
	it("publishes a substantiated official and escapes the record verbatim", async () => {
		const page = await body("/wall");
		expect(page).toContain("Dana &lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt; Reyes");
		expect(page).not.toContain("<script>alert(");
		expect(page).toContain("Demo County District Attorney");
		expect(page).toContain("SBN-100200");
		expect(page).toContain(`/wall/${ACTOR_ID}`);
		expect(page).toContain("Publication gate");
	});

	it("renders the findings, statute mapping, and score on a profile", async () => {
		const res = await get(`/wall/${ACTOR_ID}`);
		expect(res.status).toBe(200);
		const page = normalize(await res.text());
		expect(page).toContain("Brady suppression");
		expect(page).toContain("People v. Demo");
		expect(page).toContain("Witness subornation");
		// statute mapping derived from the finding types
		expect(page).toContain("18 U.S.C. § 242");
		expect(page).toContain("life authorized");
		// abuse score, shown with its formula
		expect(page).toContain("<b>46</b>");
		expect(page).toContain("min(100,");
		// referred package from the tracker
		expect(page).toContain("criminal referral");
		expect(page).toContain("presumed innocent");
	});

	it("treats a counsel hold exactly like a record that does not exist", async () => {
		const res = await get(`/wall/${HELD_ACTOR_ID}`);
		expect(res.status).toBe(404);
		const page = normalize(await res.text());
		expect(page).toContain("counsel has placed a hold");
		expect(page).toContain('name="robots" content="noindex, nofollow"');
	});

	it("filters server-side without JavaScript", async () => {
		expect(await body("/wall?jurisdiction=CA")).toContain("Dana");
		expect(await body("/wall?jurisdiction=NY")).toContain("No official matches those filters");
		expect(await body("/wall?q=zzzznomatch")).toContain("No official matches those filters");
	});
});

describe("doctrine and reference pages", () => {
	it("spells out the publication gate", async () => {
		const page = await body("/doctrine");
		expect(page).toContain("review_status = substantiated");
		expect(page).toContain("A hold removes the public card");
		expect(page).toContain("It does not charge anyone");
		expect(page).toContain("presumption of innocence");
	});

	it("advocates the statutory maximum, including life where authorized", async () => {
		const page = await body("/statutes");
		expect(page).toContain("18 U.S.C. § 1512");
		expect(page).toContain("counsel seeks the statutory maximum");
		expect(page).toContain("acted willfully");
		expect(page).toContain("Not charging decisions");
	});

	it("keeps immunity limits visible", async () => {
		const page = await body("/immunity");
		expect(page).toContain("Imbler v. Pachtman");
		expect(page).toContain("Does not bar criminal prosecution");
	});

	it("explains the correction channel", async () => {
		const page = await body("/corrections");
		expect(page).toContain("counsel@example.org");
		expect(page).toContain("Victim privacy requests are honored");
	});
});

describe("engine, tracker, ledger and search pages", () => {
	it("merges live row counts into the engine catalog", async () => {
		const page = await body("/engines");
		// Derived from the catalog: a hard-coded count silently goes stale the
		// next time an engine is added.
		expect(page).toContain(`${ENGINES.length} engines`);
		expect(page).toContain("12 rows");
		expect(page).toContain("Backend online");
	});

	it("names the post-ingest pipeline and what it may not do", async () => {
		const page = await body("/engines");
		expect(page).toContain("vi-pipeline");
		expect(page).toContain("it publishes nothing");
	});

	it("lists referred packages as drafts", async () => {
		const page = await body("/tracker");
		expect(page).toContain("Criminal referral");
		expect(page).toContain("Drafts, not filings");
	});

	it("shows the verified chain tip", async () => {
		const page = await body("/ledger");
		expect(page).toContain("verified");
		expect(page).toContain(LEDGER.tip_hash);
	});

	it("searches opinions through the backend", async () => {
		const page = await body("/cases?q=brady");
		expect(page).toContain("People v. Demo");
		expect(page).toContain("CR-2019-0001");
	});

	it("labels stored text that is only an extract", async () => {
		const page = await body("/cases?q=brady");
		expect(page).toContain("snippet");
		expect(page).toContain("badge-partial");
	});

	it("warns that a nil result over partial text proves nothing", async () => {
		// The anonymous feeds return a few hundred characters of caption page, so
		// a bare "no match" would invite the reader to conclude the case does not
		// exist. The corpus shape has to travel with the answer.
		const page = await body("/cases?q=nothingmatchesthis");
		expect(page).toContain("This searched partial text");
		expect(page).toContain("88 of 89");
		expect(page).toContain("not evidence that no such case exists");
	});

	it("drops the warning once the corpus is complete text", async () => {
		const page = await body("/cases?q=complete");
		expect(page).not.toContain("This searched partial text");
	});
});

describe("sources and provenance", () => {
	it("publishes every configured feed and its state", async () => {
		const page = await body("/sources");
		expect(page).toContain("Where these records come from");
		expect(page).toContain("brady violation");
		expect(page).toContain("CourtListener courts registry");
		expect(page).toContain("healthy");
	});

	it("publishes a failing feed rather than hiding it", async () => {
		// A gap in coverage the public cannot see looks like an absence of
		// misconduct.
		const page = await body("/sources");
		expect(page).toContain("Current feed errors");
		expect(page).toContain("429 Too Many Requests");
	});

	it("separates a feed mid-list from a feed in trouble", async () => {
		// Stopping part-way through a long list and keeping your place is
		// progress. Reported as a failure, it would understate coverage; reported
		// as healthy, it would overstate it.
		const page = await body("/sources");
		expect(page).toContain("Why a feed read less than the whole list");
		expect(page).toContain("the next poll resumes where this one stopped");
		expect(page).toContain("mid-list");
	});

	it("calls a list read to the end complete, not interrupted", async () => {
		const page = await body("/sources");
		expect(page).toContain("list complete");
		expect(page).toContain("next full crawl due");
	});

	it("names what ingestion refuses to store", async () => {
		const page = await body("/sources");
		for (const kind of ["sealed", "juvenile", "expunged"]) {
			expect(page).toContain(kind);
		}
	});

	it("says the pipeline names nobody", async () => {
		const page = await body("/sources");
		expect(page).toContain("constitution_screen");
		expect(page).toContain("names no one publicly");
	});

	it("reports names it declined to guess at", async () => {
		const page = await body("/sources");
		expect(page).toContain("2 unreadable names");
		expect(page).toContain("Nobody is named from them");
	});
});

describe("platform plumbing", () => {
	it("reports health as JSON", async () => {
		const res = await get("/healthz");
		expect(res.status).toBe(200);
		const payload = (await res.json()) as Record<string, unknown>;
		expect(payload.status).toBe("ok");
		expect(payload.charges).toBe(false);
		expect(payload.publication_gate).toContain("licensed-counsel");
	});

	it("serves immutable, content-addressed assets", async () => {
		const home = await body("/");
		const css = /href="(\/assets\/app\.[a-z0-9]+\.css)"/.exec(home)?.[1];
		const js = /src="(\/assets\/app\.[a-z0-9]+\.js)"/.exec(home)?.[1];
		expect(css).toBeTruthy();
		expect(js).toBeTruthy();
		for (const [path, type] of [
			[css as string, "text/css"],
			[js as string, "text/javascript"],
		]) {
			const res = await get(path as string);
			expect(res.status).toBe(200);
			expect(res.headers.get("content-type")).toContain(type as string);
			expect(res.headers.get("cache-control")).toContain("immutable");
		}
	});

	it("keeps crawlers off the API mirror and lists the public pages", async () => {
		const robots = await (await get("/robots.txt")).text();
		expect(robots).toContain("Disallow: /api/");
		expect(robots).toContain(`Sitemap: ${SITE}/sitemap.xml`);
		const sitemap = await (await get("/sitemap.xml")).text();
		expect(sitemap).toContain(`${SITE}/wall`);
		expect(sitemap).toContain(`${SITE}/doctrine`);
	});

	it("collapses a trailing slash into one canonical URL", async () => {
		const res = await get("/wall/");
		expect(res.status).toBe(308);
		expect(res.headers.get("location")).toBe(`${SITE}/wall`);
	});

	it("returns a helpful 404 for unknown pages", async () => {
		const res = await get("/does-not-exist");
		expect(res.status).toBe(404);
		expect(normalize(await res.text())).toContain("does not exist");
	});

	it("serves a favicon", async () => {
		const res = await SELF.fetch(`${SITE}/favicon.svg`);
		expect(res.status).toBe(200);
		expect(res.headers.get("content-type")).toContain("image/svg+xml");
	});
});
