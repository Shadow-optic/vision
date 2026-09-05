import { SELF } from "cloudflare:test";

export const SITE = "https://vision.test";

export function get(path: string, init: RequestInit = {}): Promise<Response> {
	return SELF.fetch(`${SITE}${path}`, { redirect: "manual", ...init });
}

/**
 * Fetches a page and collapses whitespace. The templates wrap prose across
 * lines, so assertions should be about the words, not the indentation.
 */
export async function body(path: string): Promise<string> {
	const res = await get(path);
	return normalize(await res.text());
}

export function normalize(html: string): string {
	return html.replace(/\s+/g, " ");
}
