import type { Ctx } from "../context";
import type { SearchResponse } from "../api-types";
import { getJson } from "../upstream";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { empty, notice, offline, unconfigured } from "../view/components";

const MAX_QUERY = 200;

export async function cases(ctx: Ctx): Promise<Response> {
	const raw = ctx.url.searchParams.get("q") ?? "";
	const q = raw.trim().slice(0, MAX_QUERY);

	const form = html`
		<form class="filters" method="get" action="/cases" role="search">
			<div class="field">
				<label for="q">Search opinions</label>
				<input
					id="q"
					name="q"
					type="search"
					value="${q}"
					placeholder="e.g. suppress exculpatory evidence"
					maxlength="${String(MAX_QUERY)}"
					autocomplete="off"
				/>
			</div>
			<div class="field">
				<button class="btn btn-primary" type="submit">Search</button>
			</div>
		</form>
	`;

	let results = html``;
	if (q !== "") {
		const data = await getJson<SearchResponse>(
			ctx.cfg,
			`/cases/search?q=${encodeURIComponent(q)}&limit=50`,
			{ ttl: 120, ctx: ctx.waitUntil },
		);
		if (data.state === "unconfigured") {
			results = unconfigured("case-law search");
		} else if (data.state !== "ok") {
			results = offline("case-law search", data.state === "error" ? data.detail : null);
		} else {
			const hits = data.data.results ?? [];
			results =
				hits.length === 0
					? empty(
							"No opinion matched",
							html`<p>
								Full-text search runs against ingested public opinions. Try
								fewer or more general terms.
							</p>`,
						)
					: html`<p class="faint" role="status">${hits.length} opinion${hits.length === 1 ? "" : "s"} matched</p>
							<div class="table-scroll">
								<table class="data">
									<thead>
										<tr>
											<th scope="col">Citation</th>
											<th scope="col">Docket</th>
											<th scope="col">Jurisdiction</th>
											<th scope="col">Issued</th>
											<th scope="col">Outcome</th>
										</tr>
									</thead>
									<tbody>
										${hits.map(
											(h) => html`<tr>
												<th scope="row">${h.citation ?? "—"}</th>
												<td class="mono faint">${h.docket_number ?? "—"}</td>
												<td>${h.jurisdiction ?? "—"}</td>
												<td>${h.date_issued ?? "—"}</td>
												<td>${h.outcome ?? "—"}</td>
											</tr>`,
										)}
									</tbody>
								</table>
							</div>`;
		}
	}

	const body = html`
		<p class="eyebrow">Case-law engine</p>
		<h1>Search the public record</h1>
		<p class="lede">
			Full-text search over opinions and dockets ingested from public court
			records. This is the evidentiary floor under every finding on the register.
		</p>
		${form} ${results}
		${q === ""
			? notice(
					"plain",
					html`<strong>What is searchable.</strong> Opinions and dockets obtained
						from public court records — nothing sealed, leaked, or purchased.
						Search results are not findings; a finding requires counsel review
						against the underlying document.`,
				)
			: ""}
	`;

	return htmlResponse(
		page(
			{
				title: q === "" ? "Case-law search" : `“${q}” — case-law search`,
				description:
					"Full-text search of public court opinions and dockets ingested by VisionInjustice.",
				path: "/cases",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/cases`,
				noindex: q !== "",
			},
			body,
		),
		{ cacheSeconds: q === "" ? 3600 : 120 },
	);
}
