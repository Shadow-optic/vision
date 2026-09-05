import type { Ctx } from "../context";
import type { EnginesResponse } from "../api-types";
import { getJson, health } from "../upstream";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { badge, notice, offline, stat, unconfigured } from "../view/components";
import { ENGINES } from "../data/engines";

export async function engines(ctx: Ctx): Promise<Response> {
	const live = await getJson<EnginesResponse>(ctx.cfg, "/engines", {
		ttl: 30,
		ctx: ctx.waitUntil,
	});

	const rowsByName = new Map<string, number>();
	if (live.state === "ok") {
		for (const e of live.data.engines) rowsByName.set(e.name, Number(e.rows || 0));
	}

	const status =
		live.state === "ok"
			? notice(
					"gate",
					html`<strong>Backend online.</strong> <code>${live.data.backend}</code>,
						database <code>${live.data.database}</code>. Row counts below are live.`,
				)
			: live.state === "unconfigured"
				? unconfigured("engine row counts")
				: offline("engine row counts", live.state === "error" ? live.detail : null);

	const total = [...rowsByName.values()].reduce((a, b) => a + b, 0);

	const body = html`
		<p class="eyebrow">Architecture</p>
		<h1>Fourteen engines</h1>
		<p class="lede">
			Every number on this site is produced by one of these engines from stored
			public records, and every action they take is appended to the Root Ledger.
			The engines analyze and draft. They do not charge, and they do not publish
			without counsel review.
		</p>

		${status}

		<section aria-label="Totals">
			<div class="grid three">
				${stat("Engines", ENGINES.length)}
				${stat("Rows analyzed", live.state === "ok" ? total : "—")}
				${stat("Backend", live.state === "ok" ? "online" : "unavailable")}
			</div>
		</section>

		<section aria-labelledby="list-h">
			<h2 id="list-h">Engines and endpoints</h2>
			<div class="grid two">
				${ENGINES.map((e) => {
					const rows = rowsByName.get(e.name);
					return html`<div class="card">
						<h3>${e.name}</h3>
						<p>
							${badge(e.crate)}
							${rows === undefined ? "" : badge(`${rows} rows`, "substantiated")}
						</p>
						<p class="muted">${e.summary}</p>
						<p class="kv">${e.routes.join("  ·  ")}</p>
					</div>`;
				})}
			</div>
		</section>

		<section aria-labelledby="api-h">
			<h2 id="api-h">Reading the engines directly</h2>
			<p class="muted">
				Public, read-only endpoints are mirrored at <a href="/api">/api</a> on this
				domain. Write operations — publication holds, package generation, rule
				runs, ingestion — exist only on the counsel-facing backend and are not
				exposed here.
			</p>
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "Engines",
				description:
					"The fourteen VisionInjustice engines, what each one does, and their live row counts.",
				path: "/engines",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/engines`,
			},
			body,
		),
		{ cacheSeconds: 30 },
	);
}

export async function statusPage(ctx: Ctx): Promise<Response> {
	const up = await health(ctx.cfg);
	const body = html`
		<p class="eyebrow">Operations</p>
		<h1>Platform status</h1>
		<div class="grid three">
			${stat("Edge", "online", "Cloudflare Workers")}
			${stat(
				"Backend",
				up.configured ? (up.reachable ? "online" : "unreachable") : "not connected",
				up.latencyMs === null ? undefined : `${up.latencyMs} ms`,
			)}
			${stat("Environment", ctx.cfg.environment)}
		</div>
		${up.detail ? notice("offline", html`<strong>Backend detail.</strong> ${up.detail}`) : ""}
		<p class="faint">
			Machine-readable status: <a href="/healthz">/healthz</a>.
		</p>
	`;
	return htmlResponse(
		page(
			{
				title: "Status",
				description: "Edge and backend availability for the VisionInjustice platform.",
				path: "/status",
				config: ctx.cfg,
				noindex: true,
			},
			body,
		),
		{ cacheSeconds: 0 },
	);
}
