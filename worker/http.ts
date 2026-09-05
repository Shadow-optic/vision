/** Response construction, security headers, and structured logging. */

const CSP = [
	"default-src 'none'",
	"base-uri 'none'",
	"form-action 'self'",
	"frame-ancestors 'none'",
	"img-src 'self' data:",
	"style-src 'self'",
	"script-src 'self'",
	"connect-src 'self'",
	"font-src 'self'",
	"manifest-src 'self'",
].join("; ");

const BASE_SECURITY: Record<string, string> = {
	"content-security-policy": CSP,
	"x-content-type-options": "nosniff",
	"referrer-policy": "strict-origin-when-cross-origin",
	"x-frame-options": "DENY",
	"permissions-policy":
		"accelerometer=(), camera=(), geolocation=(), gyroscope=(), microphone=(), payment=(), usb=()",
	"cross-origin-opener-policy": "same-origin",
};

export function harden(res: Response, url: URL, opts: { publicApi?: boolean } = {}): Response {
	const out = new Response(res.body, res);
	for (const [k, v] of Object.entries(BASE_SECURITY)) {
		if (!out.headers.has(k)) out.headers.set(k, v);
	}
	if (url.protocol === "https:") {
		out.headers.set("strict-transport-security", "max-age=31536000; includeSubDomains");
	}
	if (opts.publicApi) {
		// The JSON mirror is deliberately world-readable: radical transparency.
		out.headers.set("access-control-allow-origin", "*");
		out.headers.set("cross-origin-resource-policy", "cross-origin");
		out.headers.set("vary", "origin");
	} else {
		out.headers.set("cross-origin-resource-policy", "same-origin");
	}
	return out;
}

export function htmlResponse(
	body: string,
	init: { status?: number; cacheSeconds?: number } = {},
): Response {
	const cache =
		init.cacheSeconds === undefined || init.cacheSeconds <= 0
			? "no-store"
			: `public, max-age=${init.cacheSeconds}, stale-while-revalidate=${init.cacheSeconds * 4}`;
	return new Response(body, {
		status: init.status ?? 200,
		headers: {
			"content-type": "text/html; charset=utf-8",
			"cache-control": cache,
		},
	});
}

export function jsonResponse(
	value: unknown,
	init: { status?: number; cacheSeconds?: number } = {},
): Response {
	return new Response(`${JSON.stringify(value, null, 2)}\n`, {
		status: init.status ?? 200,
		headers: {
			"content-type": "application/json; charset=utf-8",
			"cache-control":
				init.cacheSeconds && init.cacheSeconds > 0
					? `public, max-age=${init.cacheSeconds}`
					: "no-store",
		},
	});
}

export function textResponse(
	body: string,
	init: { status?: number; contentType?: string; cacheSeconds?: number } = {},
): Response {
	return new Response(body, {
		status: init.status ?? 200,
		headers: {
			"content-type": init.contentType ?? "text/plain; charset=utf-8",
			"cache-control":
				init.cacheSeconds && init.cacheSeconds > 0
					? `public, max-age=${init.cacheSeconds}`
					: "no-store",
		},
	});
}

export interface AccessLog {
	requestId: string;
	method: string;
	path: string;
	status: number;
	durationMs: number;
	country?: string;
	rateLimited?: boolean;
	error?: string;
}

export function log(entry: AccessLog): void {
	console.log(JSON.stringify({ ts: new Date().toISOString(), ...entry }));
}

export function requestId(request: Request): string {
	const header = request.headers.get("cf-ray");
	if (header) return header;
	return crypto.randomUUID();
}
