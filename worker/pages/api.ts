import type { Ctx } from "../context";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { notice } from "../view/components";
import { ALLOWED, WITHHELD } from "../proxy";

export function apiDocs(ctx: Ctx): Response {
	const body = html`
		<p class="eyebrow">Public API</p>
		<h1>Read the same data these pages read</h1>
		<p class="lede">
			Every public page here is rendered from this JSON. It is open, unversioned
			by key, CORS-enabled, cached at the edge, and read-only:
			<code>GET</code> and <code>HEAD</code> only, no authentication, no writes.
		</p>

		${notice(
			"gate",
			html`<strong>Base URL.</strong> <code>${ctx.url.origin}/api</code> — append a
				backend path, e.g.
				<a href="/api/reckoning/wall"><code>/api/reckoning/wall</code></a>.`,
		)}

		<section aria-labelledby="allow-h">
			<h2 id="allow-h">Allowlisted endpoints</h2>
			<div class="table-scroll">
				<table class="data">
					<caption>${ALLOWED.length} public read-only endpoints.</caption>
					<thead>
						<tr>
							<th scope="col">Endpoint</th>
							<th scope="col">Returns</th>
							<th scope="col">Query</th>
							<th scope="col" class="num">Cache</th>
						</tr>
					</thead>
					<tbody>
						${ALLOWED.map(
							(r) => html`<tr>
								<th scope="row" class="mono">
									${r.pattern.includes(":")
										? html`/api${r.pattern}`
										: html`<a href="/api${r.pattern}">/api${r.pattern}</a>`}
								</th>
								<td>${r.summary}</td>
								<td class="mono faint">${(r.params ?? []).join(", ") || "—"}</td>
								<td class="num faint">${r.ttl === 0 ? "none" : `${r.ttl}s`}</td>
							</tr>`,
						)}
					</tbody>
				</table>
			</div>
		</section>

		<section aria-labelledby="withheld-h">
			<h2 id="withheld-h">What is deliberately not here</h2>
			<p class="muted">
				An accountability API that leaks unreviewed accusations would do the same
				harm it documents. These exclusions are enforced in code, not policy.
			</p>
			<div class="table-scroll">
				<table class="data">
					<thead>
						<tr>
							<th scope="col">Withheld</th>
							<th scope="col">Why</th>
						</tr>
					</thead>
					<tbody>
						${WITHHELD.map(
							(w) => html`<tr>
								<th scope="row" class="mono">${w.path}</th>
								<td>${w.reason}</td>
							</tr>`,
						)}
					</tbody>
				</table>
			</div>
		</section>

		<section aria-labelledby="use-h">
			<h2 id="use-h">Notes for consumers</h2>
			<ul class="bullets">
				<li>
					A card under a counsel hold returns <code>404</code>, identically to a
					card that never existed. Absence is not evidence either way.
				</li>
				<li>
					Unknown query parameters are dropped before the request reaches the
					backend.
				</li>
				<li>
					Errors are JSON:
					<code>{ "error": "backend_unavailable", "message": …, "path": … }</code>
					with status <code>502</code>, <code>503</code>, or <code>404</code>.
				</li>
				<li>
					Requests are rate limited per client. Exceeding the limit returns
					<code>429</code> with a <code>retry-after</code> header.
				</li>
				<li>
					Machine-readable platform status:
					<a href="/healthz"><code>/healthz</code></a>.
				</li>
			</ul>
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "Public API",
				description:
					"Read-only JSON mirror of the VisionInjustice backend: allowlisted endpoints and the reasons for every exclusion.",
				path: "/api",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/api`,
			},
			body,
		),
		{ cacheSeconds: 3600 },
	);
}
