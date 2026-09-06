/** Page shell: navigation, metadata, and the standing legal notice. */
import { CSS_PATH, JS_PATH } from "./assets";
import { html, raw, type Fragment } from "./html";
import type { SiteConfig } from "../env";

export const NAV: { href: string; label: string }[] = [
	{ href: "/wall", label: "Wall of Injustice" },
	{ href: "/statutes", label: "Statutes" },
	{ href: "/immunity", label: "Immunity" },
	{ href: "/cases", label: "Case law" },
	{ href: "/tracker", label: "Tracker" },
	{ href: "/engines", label: "Engines" },
	{ href: "/doctrine", label: "Doctrine" },
];

const FOOTER_LINKS: { href: string; label: string }[] = [
	{ href: "/doctrine", label: "Individual Accountability Doctrine" },
	{ href: "/corrections", label: "Corrections & privacy holds" },
	{ href: "/sources", label: "Sources & provenance" },
	{ href: "/ledger", label: "Root Ledger" },
	{ href: "/transparency", label: "Transparency log" },
	{ href: "/api", label: "Public API" },
	{ href: "/healthz", label: "Status" },
];

const MARK = raw(`<svg class="mark" viewBox="0 0 32 32" aria-hidden="true" focusable="false">
<path d="M16 3l11 4.4v8.8c0 6.8-4.5 11.8-11 13.2C9.5 28 5 23 5 16.2V7.4L16 3z" fill="none" stroke="#c8102e" stroke-width="2"/>
<path d="M10.6 15.6l4 4 7.2-7.4" fill="none" stroke="#d8b169" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
</svg>`);

export interface PageOptions {
	title: string;
	description: string;
	path: string;
	config: SiteConfig;
	canonical?: string;
	noindex?: boolean;
}

export function page(opts: PageOptions, body: Fragment): string {
	const fullTitle =
		opts.path === "/"
			? `${opts.config.siteName} — individual accountability for abuse of office`
			: `${opts.title} · ${opts.config.siteName}`;

	const nav = NAV.map(
		(item) => html`<li>
			<a
				href="${item.href}"
				${raw(
					opts.path === item.href || opts.path.startsWith(`${item.href}/`)
						? 'aria-current="page"'
						: "",
				)}
				>${item.label}</a
			>
		</li>`,
	);

	return `<!doctype html>
${html`<html lang="en">
	<head>
		<meta charset="utf-8" />
		<meta name="viewport" content="width=device-width, initial-scale=1" />
		<title>${fullTitle}</title>
		<meta name="description" content="${opts.description}" />
		${opts.noindex ? raw('<meta name="robots" content="noindex, nofollow" />') : ""}
		${opts.canonical ? html`<link rel="canonical" href="${opts.canonical}" />` : ""}
		<meta property="og:site_name" content="${opts.config.siteName}" />
		<meta property="og:title" content="${fullTitle}" />
		<meta property="og:description" content="${opts.description}" />
		<meta property="og:type" content="website" />
		<meta name="theme-color" content="#0b0d10" />
		<link rel="icon" href="/favicon.svg" type="image/svg+xml" />
		<link rel="stylesheet" href="${CSS_PATH}" />
	</head>
	<body>
		<a class="skip" href="#main">Skip to content</a>
		<header class="site-header">
			<div class="wrap header-row">
				<a class="brand" href="/">
					${MARK}
					<span
						>${opts.config.siteName}
						<small>No Bars Held</small>
					</span>
				</a>
				<nav class="primary" aria-label="Primary">
					<ul>
						${nav}
					</ul>
				</nav>
			</div>
		</header>
		<main id="main">
			<div class="wrap">${body}</div>
		</main>
		<footer class="site-footer">
			<div class="wrap">
				<nav aria-label="Footer">
					<ul>
						${FOOTER_LINKS.map((l) => html`<li><a href="${l.href}">${l.label}</a></li>`)}
					</ul>
				</nav>
				<p class="legal">
					<strong>Not legal advice. Not a charging document.</strong> Every record
					published here is drawn from public records and was substantiated by
					licensed counsel before publication. Pending automated flags are never
					published. This platform does not file charges — prosecutors, bar
					authorities, and courts do. Officials named here retain every due-process
					right, including the presumption of innocence in any proceeding.
				</p>
				<p class="legal">
					Corrections and victim-privacy holds:
					<a href="mailto:${opts.config.contactEmail}">${opts.config.contactEmail}</a>
					· <a href="/corrections">how review works</a>
				</p>
			</div>
		</footer>
		<script src="${JS_PATH}" defer></script>
	</body>
</html>`}`;
}
