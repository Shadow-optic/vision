/**
 * Minimal path router. Patterns use `:name` segments, e.g. `/wall/:id`.
 * Kept dependency-free so the public site has no third-party runtime code.
 */

export interface RouteMatch {
	params: Record<string, string>;
}

export type Handler<C> = (ctx: C, match: RouteMatch) => Promise<Response> | Response;

interface Route<C> {
	method: string;
	segments: string[];
	handler: Handler<C>;
}

export class Router<C> {
	private routes: Route<C>[] = [];

	get(pattern: string, handler: Handler<C>): this {
		return this.add("GET", pattern, handler);
	}

	add(method: string, pattern: string, handler: Handler<C>): this {
		this.routes.push({ method, segments: split(pattern), handler });
		return this;
	}

	/** Returns null for no path match, or `{ handler: null }` for a method mismatch. */
	lookup(
		method: string,
		path: string,
	): { handler: Handler<C>; match: RouteMatch } | { handler: null } | null {
		const segments = split(path);
		let pathMatched = false;
		for (const route of this.routes) {
			const params = matchSegments(route.segments, segments);
			if (!params) continue;
			pathMatched = true;
			const allowed = route.method === method || (route.method === "GET" && method === "HEAD");
			if (allowed) return { handler: route.handler, match: { params } };
		}
		return pathMatched ? { handler: null } : null;
	}
}

function split(path: string): string[] {
	return path.split("/").filter((s) => s !== "");
}

function matchSegments(pattern: string[], actual: string[]): Record<string, string> | null {
	if (pattern.length !== actual.length) return null;
	const params: Record<string, string> = {};
	for (let i = 0; i < pattern.length; i++) {
		const p = pattern[i] as string;
		const a = actual[i] as string;
		if (p.startsWith(":")) {
			const decoded = safeDecode(a);
			if (decoded === null) return null;
			params[p.slice(1)] = decoded;
			continue;
		}
		if (p !== a) return null;
	}
	return params;
}

function safeDecode(value: string): string | null {
	try {
		return decodeURIComponent(value);
	} catch {
		return null;
	}
}

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function isUuid(value: string | undefined): value is string {
	return typeof value === "string" && UUID_RE.test(value);
}
