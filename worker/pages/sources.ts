/**
 * Where the records come from, and what was refused.
 *
 * A register that names individuals owes the public an account of its own
 * inputs. This page publishes the feeds that are read, when each last
 * succeeded or failed, how much of the stored opinion text is complete, and
 * how many records ingestion declined to store at all.
 */
import type { Ctx } from "../context";
import type { IngestSourcesResponse, PipelineStatusResponse } from "../api-types";
import { getJson } from "../upstream";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { badge, empty, notice, offline, stat, unconfigured } from "../view/components";

const KIND_LABELS: Record<string, string> = {
	registry: "Court registry",
	search: "Search query",
	atom: "Court feed",
	api: "Authenticated API",
	fixture: "Built-in fixture",
};

export async function sources(ctx: Ctx): Promise<Response> {
	const [feeds, pipeline] = await Promise.all([
		getJson<IngestSourcesResponse>(ctx.cfg, "/ingest/sources", {
			ttl: 60,
			ctx: ctx.waitUntil,
		}),
		getJson<PipelineStatusResponse>(ctx.cfg, "/pipeline/status", {
			ttl: 60,
			ctx: ctx.waitUntil,
		}),
	]);

	const intro = html`
		<p class="eyebrow">Provenance</p>
		<h1>Where these records come from</h1>
		<p class="lede">
			Every record on this platform was published by a court or another public
			body. Below are the feeds that are read, when each last succeeded, and what
			ingestion refused to store.
		</p>
		${notice(
			"plain",
			html`<strong>Ingestion concludes nothing.</strong> Reading a public record
				is not an allegation about anyone named in it. Which feeds are polled
				decides where we look first; it decides nothing about what is found.`,
		)}
	`;

	if (feeds.state === "unconfigured") {
		return render(ctx, html`${intro}${unconfigured("ingestion sources")}`);
	}
	if (feeds.state !== "ok") {
		return render(
			ctx,
			html`${intro}${offline("ingestion sources", feeds.state === "error" ? feeds.detail : null)}`,
		);
	}

	const list = feeds.data.sources ?? [];
	const configured = list.filter((s) => s.configured);
	const failing = configured.filter((s) => (s.status?.consecutive_failures ?? 0) > 0);
	const skipped = configured.reduce((sum, s) => sum + (s.status?.total_skipped ?? 0), 0);

	const feedTable =
		list.length === 0
			? empty(
					"No feed is configured",
					html`<p>This deployment reads no public feed yet.</p>`,
				)
			: html`<div class="table-scroll">
					<table class="data">
						<thead>
							<tr>
								<th scope="col">Feed</th>
								<th scope="col">Kind</th>
								<th scope="col">Last success</th>
								<th scope="col">Records</th>
								<th scope="col">Refused</th>
								<th scope="col">State</th>
							</tr>
						</thead>
						<tbody>
							${list.map((s) => {
								const st = s.status;
								return html`<tr>
									<th scope="row">${s.label ?? s.source}</th>
									<td>${KIND_LABELS[st?.feed_kind ?? ""] ?? st?.feed_kind ?? "—"}</td>
									<td class="faint">${st?.last_ok_at ? st.last_ok_at.slice(0, 19).replace("T", " ") : "never"}</td>
									<td>${st ? `${st.new_cases ?? 0} / ${st.new_opinions ?? 0}` : "—"}</td>
									<td>${st?.total_skipped ?? 0}</td>
									<td>
										${feedState(
											s.configured,
											st?.consecutive_failures ?? 0,
											st?.last_pause ?? null,
										)}
									</td>
								</tr>`;
							})}
						</tbody>
					</table>
				</div>`;

	const errors = failing
		.map((s) => ({ label: s.label ?? s.source, detail: s.status?.last_error }))
		.filter((e) => e.detail);

	// A feed part-way through a long list is not a feed in trouble, and the two
	// read very differently to someone judging whether coverage can be trusted.
	const paused = configured
		.filter((s) => (s.status?.consecutive_failures ?? 0) === 0)
		.map((s) => ({ label: s.label ?? s.source, detail: s.status?.last_pause }))
		.filter((s) => s.detail);

	const pipelineSection =
		pipeline.state === "ok"
			? html`
					<section aria-labelledby="pipe-h">
						<h2 id="pipe-h">What happens after a record arrives</h2>
						<p class="muted">
							Each ingested record is walked through
							${(pipeline.data.stages ?? []).join(", ")}. Every artifact this
							produces is pending: it names no one publicly and moves no
							official's score.
						</p>
						<div class="grid three">
							${stat("Records processed", pipeline.data.cases?.total ?? 0)}
							${stat("Awaiting processing", pipeline.data.cases?.awaiting_pipeline ?? 0)}
							${stat("Constitutional screens", pipeline.data.totals?.screens ?? 0)}
						</div>
						${unresolvedNotice(pipeline.data)}
					</section>
				`
			: "";

	const body = html`
		${intro}
		<section aria-label="Totals">
			<div class="grid three">
				${stat("Feeds configured", configured.length)}
				${stat("Feeds erroring", failing.length)}
				${stat("Records refused", skipped)}
			</div>
		</section>

		<section aria-labelledby="feeds-h">
			<h2 id="feeds-h">Feeds</h2>
			<p class="muted">
				Records counts are new cases and new opinions this feed was the first to
				bring in. A live feed re-reads its own head continuously, so counting
				every read would say nothing.
			</p>
			${feedTable}
		</section>

		${errors.length > 0
			? html`<section aria-labelledby="err-h">
					<h2 id="err-h">Current feed errors</h2>
					<p class="muted">
						Published rather than hidden: a feed that is failing is a gap in
						coverage, and a gap the public cannot see is a gap that looks like an
						absence of misconduct.
					</p>
					${errors.map((e) =>
						notice("offline", html`<strong>${e.label}.</strong> ${e.detail}`),
					)}
				</section>`
			: ""}

		${paused.length > 0
			? html`<section aria-labelledby="pause-h">
					<h2 id="pause-h">Feeds part-way through a list</h2>
					<p class="muted">
						These feeds stopped before the end of the source's list and kept
						their place, so the next poll continues rather than starting over.
						The records already read are stored; the rest have not been read
						yet.
					</p>
					${paused.map((s) =>
						notice("plain", html`<strong>${s.label}.</strong> ${s.detail}`),
					)}
				</section>`
			: ""}

		<section aria-labelledby="refused-h">
			<h2 id="refused-h">What ingestion refuses</h2>
			<p class="muted">
				${skipped} record${skipped === 1 ? " has" : "s have"} been offered by a
				source and not stored. These categories are never ingested regardless of
				what a feed publishes:
			</p>
			<p>${(feeds.data.exclusions ?? []).map((x) => badge(x))}</p>
		</section>

		${pipelineSection}
	`;

	return render(ctx, body);
}

function feedState(configured: boolean, failures: number, pause: string | null) {
	if (!configured) return badge("not configured");
	if (failures > 0) {
		return badge(`${failures} failure${failures === 1 ? "" : "s"}`, "referred");
	}
	if (pause) return badge("mid-list", "partial");
	return badge("healthy", "substantiated");
}

/**
 * Judge fields that named no readable individual.
 *
 * Worth publishing: it is the count of records where the platform declined to
 * decide who an official was rather than guessing.
 */
function unresolvedNotice(data: PipelineStatusResponse) {
	const open = data.unresolved_officials?.open_ambiguous ?? 0;
	if (open === 0) return "";
	return notice(
		"gate",
		html`<strong>${String(open)} unreadable name${open === 1 ? "" : "s"}.</strong>
			A court record can name a panel of judges in one field without saying where
			one name ends and the next begins. Rather than guess — and risk putting one
			official's conduct on another official's record — these are held for a
			person to resolve. Nobody is named from them in the meantime.`,
	);
}

function render(ctx: Ctx, body: ReturnType<typeof html>): Response {
	return htmlResponse(
		page(
			{
				title: "Sources",
				description:
					"The public feeds VisionInjustice reads, their current state, and the records ingestion refuses to store.",
				path: "/sources",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/sources`,
			},
			body,
		),
		{ cacheSeconds: 60 },
	);
}
