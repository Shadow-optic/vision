/**
 * VisionInjustice public platform — Cloudflare Worker.
 *
 * Serves the public accountability site and a read-only JSON mirror of the
 * Rust `vi-api` backend. Nothing on this edge can publish, hold, or generate a
 * legal package: those are counsel operations and live only on the backend.
 */
export { MyWorkflow } from "./workflow";
export { WorkflowStatusDO } from "./durable-object";

import { config, type Env } from "./env";
import type { Ctx } from "./context";
import { harden, jsonResponse, log, requestId, textResponse } from "./http";
import { Router } from "./router";
import { assetResponse } from "./view/assets";
import { proxy } from "./proxy";
import { health } from "./upstream";
import { home } from "./pages/home";
import { wallIndex, wallProfile } from "./pages/wall";
import { doctrine, corrections } from "./pages/doctrine";
import { statutes, immunity } from "./pages/statutes";
import { engines, statusPage } from "./pages/engines";
import { tracker, ledger } from "./pages/tracker";
import { transparency } from "./pages/transparency";
import { cases } from "./pages/cases";
import { sources } from "./pages/sources";
import { apiDocs } from "./pages/api";
import { methodNotAllowed, notFound, serverError } from "./pages/errors";

const PUBLIC_PAGES = [
	"/",
	"/wall",
	"/statutes",
	"/immunity",
	"/cases",
	"/sources",
	"/tracker",
	"/engines",
	"/doctrine",
	"/corrections",
	"/ledger",
	"/transparency",
	"/api",
];

const router = new Router<Ctx>()
	.get("/", (ctx) => home(ctx))
	.get("/wall", (ctx) => wallIndex(ctx))
	.get("/wall/:id", (ctx, m) => wallProfile(ctx, m.params.id as string))
	.get("/doctrine", (ctx) => doctrine(ctx))
	.get("/corrections", (ctx) => corrections(ctx))
	.get("/statutes", (ctx) => statutes(ctx))
	.get("/immunity", (ctx) => immunity(ctx))
	.get("/engines", (ctx) => engines(ctx))
	.get("/status", (ctx) => statusPage(ctx))
	.get("/tracker", (ctx) => tracker(ctx))
	.get("/ledger", (ctx) => ledger(ctx))
	.get("/transparency", (ctx) => transparency(ctx))
	.get("/cases", (ctx) => cases(ctx))
	.get("/sources", (ctx) => sources(ctx))
	.get("/api", (ctx) => apiDocs(ctx))
	.get("/healthz", (ctx) => healthz(ctx))
	.get("/robots.txt", (ctx) => robots(ctx))
	.get("/sitemap.xml", (ctx) => sitemap(ctx));

async function healthz(ctx: Ctx): Promise<Response> {
	const up = await health(ctx.cfg);
	const ok = !up.configured || up.reachable;
	return jsonResponse(
		{
			status: ok ? "ok" : "degraded",
			edge: "ok",
			environment: ctx.cfg.environment,
			backend: {
				configured: up.configured,
				reachable: up.reachable,
				status: up.status,
				latency_ms: up.latencyMs,
				detail: up.detail,
			},
			charges: false,
			publication_gate: "licensed-counsel substantiation of public-record findings",
		},
		{ status: ok ? 200 : 503 },
	);
}

function robots(ctx: Ctx): Response {
	const body = [
		"User-agent: *",
		"Allow: /",
		"Disallow: /api/",
		"Disallow: /healthz",
		"Disallow: /status",
		"",
		`Sitemap: ${ctx.url.origin}/sitemap.xml`,
		"",
	].join("\n");
	return textResponse(body, { cacheSeconds: 86400 });
}

function sitemap(ctx: Ctx): Response {
	const urls = PUBLIC_PAGES.map(
		(p) => `  <url><loc>${ctx.url.origin}${p}</loc></url>`,
	).join("\n");
	const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>
`;
	return textResponse(body, {
		contentType: "application/xml; charset=utf-8",
		cacheSeconds: 86400,
	});
}

function rateLimitKey(request: Request, bucket: string): string {
	const ip = request.headers.get("cf-connecting-ip") ?? "unknown";
	return `${bucket}:${ip}`;
}

async function limited(
	limiter: { limit(o: { key: string }): Promise<{ success: boolean }> } | undefined,
	request: Request,
	bucket: string,
): Promise<boolean> {
	if (!limiter) return false;
	try {
		const { success } = await limiter.limit({ key: rateLimitKey(request, bucket) });
		return !success;
	} catch {
		// A limiter failure must never take the site down.
		return false;
	}
}

function tooMany(isApi: boolean): Response {
	if (isApi) {
		const res = jsonResponse(
			{ error: "rate_limited", message: "Too many requests. Retry in a minute." },
			{ status: 429 },
		);
		res.headers.set("retry-after", "60");
		return res;
	}
	const res = textResponse("Too many requests. Retry in a minute.\n", { status: 429 });
	res.headers.set("retry-after", "60");
	return res;
}

export default {
	async fetch(request: Request, env: Env, executionCtx: ExecutionContext): Promise<Response> {
		const started = Date.now();
		const url = new URL(request.url);
		const rid = requestId(request);
		const cfg = config(env);
		const isApi = url.pathname === "/api" ? false : url.pathname.startsWith("/api/");

		const finish = (res: Response, extra: Partial<{ rateLimited: boolean; error: string }> = {}) => {
			const hardened = harden(res, url, { publicApi: isApi });
			hardened.headers.set("x-request-id", rid);
			log({
				requestId: rid,
				method: request.method,
				path: url.pathname,
				status: hardened.status,
				durationMs: Date.now() - started,
				country: request.headers.get("cf-ipcountry") ?? undefined,
				...extra,
			});
			return request.method === "HEAD"
				? new Response(null, {
						status: hardened.status,
						statusText: hardened.statusText,
						headers: hardened.headers,
					})
				: hardened;
		};

		try {
			if (request.method !== "GET" && request.method !== "HEAD") {
				if (isApi) {
					const res = jsonResponse(
						{
							error: "read_only",
							message:
								"The public mirror is read-only. Publication, holds, and package generation are counsel operations on the backend.",
						},
						{ status: 405 },
					);
					res.headers.set("allow", "GET, HEAD");
					return finish(res);
				}
				const ctx: Ctx = { request, url, env, cfg, waitUntil: executionCtx };
				return finish(methodNotAllowed(ctx));
			}

			const asset = assetResponse(url.pathname);
			if (asset) return finish(asset);

			if (isApi) {
				if (await limited(env.API_LIMITER, request, "api")) {
					return finish(tooMany(true), { rateLimited: true });
				}
				const backendPath = url.pathname.slice("/api".length) || "/";
				return finish(await proxy(cfg, url, backendPath));
			}

			if (await limited(env.PAGE_LIMITER, request, "page")) {
				return finish(tooMany(false), { rateLimited: true });
			}

			// Normalize a trailing slash so `/wall/` and `/wall` are one URL.
			if (url.pathname.length > 1 && url.pathname.endsWith("/")) {
				const target = new URL(url);
				target.pathname = url.pathname.replace(/\/+$/, "");
				return finish(Response.redirect(target.toString(), 308));
			}

			const ctx: Ctx = { request, url, env, cfg, waitUntil: executionCtx };
			const found = router.lookup(request.method, url.pathname);
			if (found === null) return finish(notFound(ctx));
			if (found.handler === null) return finish(methodNotAllowed(ctx));
			return finish(await found.handler(ctx, found.match));
		} catch (err) {
			const detail = err instanceof Error ? `${err.name}: ${err.message}` : "unknown error";
			if (isApi) {
				return finish(
					jsonResponse({ error: "internal_error", request_id: rid }, { status: 500 }),
					{ error: detail },
				);
			}
			return finish(serverError(cfg, url.pathname, rid), { error: detail });
		}
	},
} satisfies ExportedHandler<Env>;
