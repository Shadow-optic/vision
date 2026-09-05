/** Bindings and configuration for the public VisionInjustice platform. */

export interface RateLimiter {
	limit(options: { key: string }): Promise<{ success: boolean }>;
}

export interface Env {
	/** Origin of the Rust `vi-api` service, e.g. `https://api.example.org`. */
	VI_API_ORIGIN?: string;
	/** Optional bearer token forwarded to the backend for read endpoints. */
	VI_API_TOKEN?: string;
	SITE_NAME?: string;
	CONTACT_EMAIL?: string;
	ENVIRONMENT?: string;
	PAGE_LIMITER?: RateLimiter;
	API_LIMITER?: RateLimiter;
	MY_WORKFLOW?: unknown;
	WORKFLOW_STATUS?: unknown;
}

export interface SiteConfig {
	siteName: string;
	contactEmail: string;
	environment: string;
	/** Normalized backend origin without a trailing slash, or null when unset. */
	apiOrigin: string | null;
	apiToken: string | null;
}

/** A backend origin is only accepted over HTTPS (or localhost for `wrangler dev`). */
function normalizeOrigin(raw: string | undefined): string | null {
	const value = (raw ?? "").trim();
	if (value === "") return null;
	let url: URL;
	try {
		url = new URL(value);
	} catch {
		return null;
	}
	const local =
		url.hostname === "localhost" ||
		url.hostname === "127.0.0.1" ||
		url.hostname.endsWith(".localhost");
	if (url.protocol !== "https:" && !(url.protocol === "http:" && local)) {
		return null;
	}
	return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
}

export function config(env: Env): SiteConfig {
	return {
		siteName: env.SITE_NAME?.trim() || "VisionInjustice",
		contactEmail: env.CONTACT_EMAIL?.trim() || "counsel@visioninjustice.org",
		environment: env.ENVIRONMENT?.trim() || "production",
		apiOrigin: normalizeOrigin(env.VI_API_ORIGIN),
		apiToken: env.VI_API_TOKEN?.trim() || null,
	};
}
