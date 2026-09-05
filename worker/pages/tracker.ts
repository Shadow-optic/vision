import type { Ctx } from "../context";
import type { LedgerVerifyResponse, TrackerResponse } from "../api-types";
import { getJson } from "../upstream";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { badge, empty, notice, offline, stat, unconfigured } from "../view/components";

const KIND_LABELS: Record<string, string> = {
	criminal_referral: "Criminal referral",
	civil_1983: "§ 1983 civil action",
	bar_complaint: "Bar complaint",
	sentencing_memo: "Sentencing memorandum",
};

export async function tracker(ctx: Ctx): Promise<Response> {
	const data = await getJson<TrackerResponse>(ctx.cfg, "/reckoning/tracker", {
		ttl: 60,
		ctx: ctx.waitUntil,
	});

	const intro = html`
		<p class="eyebrow">Accountability tracker</p>
		<h1>Referrals and legal action</h1>
		<p class="lede">
			Attorney-reviewed work product generated for officials on the register:
			criminal referral drafts, bar complaints, § 1983 scaffolds, and — only after
			a conviction — sentencing memoranda seeking the statutory maximum.
		</p>
		${notice(
			"plain",
			html`<strong>Drafts, not filings.</strong> Every package is attorney work
				product prepared for a licensed attorney's signature. This platform files
				nothing. Prosecutors, bar authorities, and courts decide what happens
				next.`,
		)}
	`;

	if (data.state === "unconfigured") {
		return render(ctx, html`${intro}${unconfigured("tracker data")}`);
	}
	if (data.state !== "ok") {
		return render(
			ctx,
			html`${intro}${offline("tracker data", data.state === "error" ? data.detail : null)}`,
		);
	}

	const rows = data.data.packages ?? [];
	const referred = rows.filter((r) => r.status === "referred").length;
	const kinds = new Set(rows.map((r) => r.action_kind)).size;

	const body = html`
		${intro}
		<section aria-label="Totals">
			<div class="grid three">
				${stat("Packages", rows.length)} ${stat("Referred", referred)}
				${stat("Action types", kinds)}
			</div>
		</section>
		<section aria-labelledby="rows-h">
			<h2 id="rows-h">Packages</h2>
			${rows.length === 0
				? empty(
						"No package has been referred yet",
						html`<p>
							Packages appear here once counsel has reviewed them and the
							official's underlying findings are substantiated and unheld.
						</p>`,
					)
				: html`<div class="table-scroll">
						<table class="data" id="tracker-table">
							<thead>
								<tr>
									<th scope="col">Official</th>
									<th scope="col">Action</th>
									<th scope="col">Status</th>
									<th scope="col">Package</th>
								</tr>
							</thead>
							<tbody>
								${rows.map(
									(r) => html`<tr>
										<th scope="row"><a href="/wall/${r.actor_id}">${r.display_name}</a></th>
										<td>${KIND_LABELS[r.action_kind] ?? r.action_kind.replace(/_/g, " ")}</td>
										<td>
											${badge(r.status, r.status === "referred" ? "referred" : "substantiated")}
										</td>
										<td class="mono faint">${r.package_id}</td>
									</tr>`,
								)}
							</tbody>
						</table>
					</div>`}
		</section>
	`;

	return render(ctx, body);
}

function render(ctx: Ctx, body: ReturnType<typeof html>): Response {
	return htmlResponse(
		page(
			{
				title: "Tracker",
				description:
					"Criminal referrals, bar complaints, § 1983 scaffolds, and sentencing memoranda prepared for counsel.",
				path: "/tracker",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/tracker`,
			},
			body,
		),
		{ cacheSeconds: 60 },
	);
}

export async function ledger(ctx: Ctx): Promise<Response> {
	const data = await getJson<LedgerVerifyResponse>(ctx.cfg, "/ledger/verify", {
		ttl: 30,
		ctx: ctx.waitUntil,
	});

	const intro = html`
		<p class="eyebrow">Root Ledger</p>
		<h1>Verify the record of what we did</h1>
		<p class="lede">
			Every ingest, rule run, counsel review, score snapshot, generated package,
			and publication decision is appended to a BLAKE3 hash-chained, append-only
			ledger. Each entry commits to the hash of the entry before it, so altering
			any past entry breaks every hash after it.
		</p>
	`;

	const detail =
		data.state === "ok"
			? html`
					<section aria-label="Chain state">
						<div class="grid three">
							${stat("Entries", data.data.entries)}
							${stat("Chain", data.data.ok ? "verified" : "broken")}
							${stat("First bad seq", data.data.first_bad_seq ?? "none")}
						</div>
					</section>
					<section aria-labelledby="tip-h">
						<h2 id="tip-h">Chain tip</h2>
						<pre>${data.data.tip_hash ?? "genesis"}</pre>
						<p class="faint">
							Re-verify at any time: <code>GET /api/ledger/verify</code>. The
							backend recomputes the whole chain, it does not trust a stored flag.
						</p>
					</section>
				`
			: data.state === "unconfigured"
				? unconfigured("ledger verification")
				: offline("ledger verification", data.state === "error" ? data.detail : null);

	const body = html`
		${intro}${detail}
		<section aria-labelledby="why-h">
			<h2 id="why-h">Why an accountability platform must be auditable itself</h2>
			<p class="muted">
				This project accuses officials of altering, withholding, and rewriting the
				record. It would be worthless if its own record could be quietly edited.
				The ledger means a published finding, a hold, or a withdrawal can be
				proven to have happened when it is claimed to have happened — including
				against us.
			</p>
			${notice(
				"gate",
				html`<strong>Scope.</strong> The ledger proves the integrity of this
					platform's own actions. It is not evidence that a documented finding is
					true; the public-record citation on each card is.`,
			)}
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "Root Ledger",
				description:
					"Verify the BLAKE3 hash-chained, append-only ledger of every action the platform has taken.",
				path: "/ledger",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/ledger`,
			},
			body,
		),
		{ cacheSeconds: 30 },
	);
}
