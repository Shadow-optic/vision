/** Shared page furniture. */
import { html, raw, type Fragment, type Renderable } from "./html";

export function stat(label: string, value: Renderable, note?: string): Fragment {
	return html`<div class="stat">
		<div class="n">${value}</div>
		<div class="l">${label}</div>
		${note ? html`<div class="faint">${note}</div>` : ""}
	</div>`;
}

export function badge(text: string, variant?: string): Fragment {
	return html`<span class="badge ${variant ? `badge-${variant}` : ""}">${text}</span>`;
}

export function notice(kind: "gate" | "offline" | "plain", body: Fragment): Fragment {
	const cls = kind === "plain" ? "notice" : `notice notice-${kind}`;
	return html`<p class="${cls}">${body}</p>`;
}

export function empty(title: string, body: Fragment): Fragment {
	return html`<div class="empty">
		<h3>${title}</h3>
		${body}
	</div>`;
}

export function breadcrumb(trail: { href?: string; label: string }[]): Fragment {
	const parts: Renderable[] = [];
	trail.forEach((item, i) => {
		if (i > 0) parts.push(raw(" &rsaquo; "));
		parts.push(item.href ? html`<a href="${item.href}">${item.label}</a>` : html`${item.label}`);
	});
	return html`<nav class="breadcrumb" aria-label="Breadcrumb">${parts}</nav>`;
}

/** Renders the standard degraded state when the backend is unreachable. */
export function offline(what: string, detail: string | null): Fragment {
	return notice(
		"offline",
		html`<strong>Live ${what} is unavailable right now.</strong> This page reads
			from the <code>vi-api</code> service, which is not answering. Nothing is
			cached or approximated here on purpose — an accountability register must
			never display a number it cannot source.
			${detail ? html` <span class="faint mono">(${detail})</span>` : ""}`,
	);
}

/** Backend not configured yet — distinct from an outage. */
export function unconfigured(what: string): Fragment {
	return notice(
		"offline",
		html`<strong>Live ${what} is not connected in this environment.</strong> Set
			the <code>VI_API_ORIGIN</code> variable on this Worker to the
			<code>vi-api</code> origin and the page will populate from public records
			and counsel-substantiated findings. Doctrine, statute, and immunity pages
			are served from the Worker itself and are complete.`,
	);
}

export function scoreMeter(value: number): Fragment {
	const pct = Math.max(0, Math.min(100, Math.round(value)));
	const bucket = Math.round(pct / 5) * 5;
	return html`<div>
		<span class="score"><b>${pct}</b><span class="faint">/100</span></span>
		<div class="meter" role="img" aria-label="Abuse score ${pct} out of 100">
			<span class="w${bucket}"></span>
		</div>
	</div>`;
}
