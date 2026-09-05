/**
 * Client for the Rust `vi-api` service.
 *
 * Three failure modes are distinguished on purpose, because a public
 * accountability register must never blur them: the backend is not connected
 * in this environment, the backend is unreachable/erroring, or the record
 * genuinely does not exist (or is under a counsel hold, which is a 404 by
 * design). Results are cached briefly in the Cloudflare cache; nothing is
 * ever synthesized.
 */
import type { SiteConfig } from "./env";

export type Fetched<T> =
	| { state: "ok"; data: T; cached: boolean }
	| { state: "unconfigured" }
	| { state: "missing" }
	| { state: "error"; detail: string };

export interface FetchOptions {
	/** Seconds to keep the JSON in the edge cache. 0 disables caching. */
	ttl?: number;
	timeoutMs?: number;
	ctx?: { waitUntil(p: Promise<unknown>): void };
}

const DEFAULT_TIMEOUT_MS = 6000;

export function isConfigured(cfg: SiteConfig): boolean {
	return cfg.apiOrigin !== null;
}

export function upstreamUrl(cfg: SiteConfig, path: string): string | null {
	if (!cfg.apiOrigin) return null;
	return `${cfg.apiOrigin}${path.startsWith("/") ? path : `/${path}`}`;
}

/** Raw pass-through used by the JSON proxy; callers add their own caching. */
export async function requestUpstream(
	cfg: SiteConfig,
	path: string,
	timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<Response | null> {
	const url = upstreamUrl(cfg, path);
	if (!url) return null;
	const headers: Record<string, string> = { accept: "application/json" };
	if (cfg.apiToken) headers.authorization = `Bearer ${cfg.apiToken}`;
	const res = await fetch(url, {
		method: "GET",
		headers,
		// Workers does not implement `redirect: "error"`. Following a redirect
		// would let a misconfigured origin bounce a read somewhere else, so a 3xx
		// is treated as a backend failure instead.
		redirect: "manual",
		signal: AbortSignal.timeout(timeoutMs),
	});
	if (res.status >= 300 && res.status < 400) {
		throw new Error(`upstream redirected (${res.status})`);
	}
	return res;
}

export async function getJson<T>(
	cfg: SiteConfig,
	path: string,
	opts: FetchOptions = {},
): Promise<Fetched<T>> {
	const url = upstreamUrl(cfg, path);
	if (!url) return { state: "unconfigured" };

	const ttl = opts.ttl ?? 30;
	const cacheKey = new Request(`https://cache.visioninjustice.internal${path}`, {
		method: "GET",
	});
	const cache = caches.default;

	if (ttl > 0) {
		const hit = await cache.match(cacheKey);
		if (hit) {
			try {
				return { state: "ok", data: (await hit.json()) as T, cached: true };
			} catch {
				// fall through to a live read
			}
		}
	}

	let res: Response;
	try {
		const r = await requestUpstream(cfg, path, opts.timeoutMs ?? DEFAULT_TIMEOUT_MS);
		if (!r) return { state: "unconfigured" };
		res = r;
	} catch (err) {
		return { state: "error", detail: describe(err) };
	}

	if (res.status === 404) return { state: "missing" };
	if (!res.ok) return { state: "error", detail: `upstream ${res.status}` };

	let text: string;
	try {
		text = await res.text();
	} catch (err) {
		return { state: "error", detail: describe(err) };
	}

	let data: T;
	try {
		data = JSON.parse(text) as T;
	} catch {
		return { state: "error", detail: "upstream returned malformed JSON" };
	}

	if (ttl > 0) {
		const stored = new Response(text, {
			headers: {
				"content-type": "application/json; charset=utf-8",
				"cache-control": `public, max-age=${ttl}`,
			},
		});
		const put = cache.put(cacheKey, stored);
		if (opts.ctx) opts.ctx.waitUntil(put);
		else await put;
	}

	return { state: "ok", data, cached: false };
}

export interface UpstreamHealth {
	configured: boolean;
	reachable: boolean;
	status: number | null;
	latencyMs: number | null;
	detail: string | null;
}

export async function health(cfg: SiteConfig): Promise<UpstreamHealth> {
	if (!cfg.apiOrigin) {
		return {
			configured: false,
			reachable: false,
			status: null,
			latencyMs: null,
			detail: "VI_API_ORIGIN is not set",
		};
	}
	const started = Date.now();
	try {
		const res = await requestUpstream(cfg, "/health", 3000);
		if (!res) throw new Error("no upstream");
		return {
			configured: true,
			reachable: res.ok,
			status: res.status,
			latencyMs: Date.now() - started,
			detail: res.ok ? null : `upstream ${res.status}`,
		};
	} catch (err) {
		return {
			configured: true,
			reachable: false,
			status: null,
			latencyMs: Date.now() - started,
			detail: describe(err),
		};
	}
}

function describe(err: unknown): string {
	if (err instanceof Error) {
		return err.name === "TimeoutError" ? "upstream timed out" : err.message;
	}
	return "upstream request failed";
}
