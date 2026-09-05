/** Immutable, content-addressed static assets. Keeps the CSP free of inline code. */
import { CSS } from "./styles";
import { JS } from "./client";

/** djb2 — enough to bust a cache, never used for integrity. */
function digest(text: string): string {
	let h = 5381;
	for (let i = 0; i < text.length; i++) {
		h = ((h << 5) + h + text.charCodeAt(i)) | 0;
	}
	return (h >>> 0).toString(36);
}

export const CSS_PATH = `/assets/app.${digest(CSS)}.css`;
export const JS_PATH = `/assets/app.${digest(JS)}.js`;

export const FAVICON = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
<rect width="32" height="32" rx="6" fill="#0b0d10"/>
<path d="M16 4l10 4v8c0 6.2-4.1 10.7-10 12C10.1 26.7 6 22.2 6 16V8l10-4z" fill="none" stroke="#c8102e" stroke-width="2"/>
<path d="M11 15.5l3.6 3.6L21.5 12" fill="none" stroke="#d8b169" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
</svg>`;

const IMMUTABLE = "public, max-age=31536000, immutable";

export function assetResponse(path: string): Response | null {
	if (path === CSS_PATH) {
		return new Response(CSS, {
			headers: { "content-type": "text/css; charset=utf-8", "cache-control": IMMUTABLE },
		});
	}
	if (path === JS_PATH) {
		return new Response(JS, {
			headers: {
				"content-type": "text/javascript; charset=utf-8",
				"cache-control": IMMUTABLE,
			},
		});
	}
	if (path === "/favicon.svg" || path === "/favicon.ico") {
		return new Response(FAVICON, {
			headers: {
				"content-type": "image/svg+xml; charset=utf-8",
				"cache-control": "public, max-age=86400",
			},
		});
	}
	return null;
}
