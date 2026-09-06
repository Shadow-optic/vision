import type { Ctx } from "../context";
import type { TransparencySnapshotsResponse } from "../api-types";
import { getJson } from "../upstream";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { empty, notice, offline, stat, unconfigured } from "../view/components";

/** Human label for each leaf table the snapshot commits to. */
const TABLE_LABELS: Record<string, string> = {
	__chain__: "Chain (previous root + ledger tip)",
	findings: "Substantiated findings",
	ledger_events: "Ledger events",
	referrals: "Referred packages",
	statutes: "Statute catalog",
	wall_entries: "Published register entries",
};

export async function transparency(ctx: Ctx): Promise<Response> {
	const data = await getJson<TransparencySnapshotsResponse>(ctx.cfg, "/transparency/snapshots", {
		ttl: 60,
		ctx: ctx.waitUntil,
	});

	const intro = html`
		<p class="eyebrow">Transparency proof log</p>
		<h1>Prove the published record has not changed</h1>
		<p class="lede">
			Periodically, the backend hashes every published row — register entries,
			substantiated findings, referred packages, the statute catalog, and the
			ledger itself — into a Merkle tree whose root is anchored in the Root
			Ledger. Any past snapshot can be re-verified; any quiet edit breaks the
			root. Pending, machine-derived leads are never part of a snapshot.
		</p>
		${notice(
			"plain",
			html`<strong>Audit, not accusation.</strong> A snapshot authenticates what was
				already public at capture time. It adds nothing to the record and accuses
				no one.`,
		)}
	`;

	if (data.state === "unconfigured") {
		return render(ctx, html`${intro}${unconfigured("transparency snapshots")}`);
	}
	if (data.state !== "ok") {
		return render(
			ctx,
			html`${intro}${offline(
				"transparency snapshots",
				data.state === "error" ? data.detail : null,
			)}`,
		);
	}

	const snaps = data.data.snapshots ?? [];
	const latest = snaps[0];

	const summary = latest
		? html`
				<section aria-label="Latest snapshot">
					<div class="grid three">
						${stat("Snapshots", snaps.length)}
						${stat("Leaves in latest tree", latest.tree_size)}
						${stat("Ledger tip pinned", latest.ledger_seq ?? "genesis")}
					</div>
					<h2>Latest Merkle root</h2>
					<pre>${latest.merkle_root}</pre>
					<p class="faint">
						Captured ${new Date(latest.created_at).toUTCString()}. Verify any
						published row against this root with
						<code>GET /api/transparency/proof/&lt;table&gt;/&lt;row_id&gt;</code>, and
						verify the ledger that anchors it on the
						<a href="/ledger">Root Ledger page</a>.
					</p>
					<h2>What the latest snapshot commits to</h2>
					<div class="table-scroll">
						<table class="data">
							<thead>
								<tr><th scope="col">Table</th><th scope="col">Rows</th></tr>
							</thead>
							<tbody>
								${Object.entries(latest.table_counts).map(
									([table, count]) => html`<tr>
										<th scope="row">${TABLE_LABELS[table] ?? table}</th>
										<td>${count}</td>
									</tr>`,
								)}
							</tbody>
						</table>
					</div>
				</section>
			`
		: empty(
				"No snapshot taken yet",
				html`<p>
					Snapshots are captured on demand by an operator. Once one exists, its
					root, table counts, and pinned ledger tip appear here.
				</p>`,
			);

	const history =
		snaps.length === 0
			? html``
			: html`
					<section aria-labelledby="history-h">
						<h2 id="history-h">Snapshot history</h2>
						<div class="table-scroll">
							<table class="data">
								<thead>
									<tr>
										<th scope="col">Captured</th>
										<th scope="col">Merkle root</th>
										<th scope="col">Leaves</th>
										<th scope="col">Ledger seq</th>
										<th scope="col">Chained from</th>
									</tr>
								</thead>
								<tbody>
									${snaps.map(
										(s) => html`<tr>
											<td>${new Date(s.created_at).toUTCString()}</td>
											<td class="mono faint">${s.merkle_root.slice(0, 16)}…</td>
											<td>${s.tree_size}</td>
											<td>${s.ledger_seq ?? "genesis"}</td>
											<td class="mono faint">
												${s.prev_root ? `${s.prev_root.slice(0, 16)}…` : "genesis"}
											</td>
										</tr>`,
									)}
								</tbody>
							</table>
						</div>
					</section>
				`;

	return render(ctx, html`${intro}${summary}${history}`);
}

function render(ctx: Ctx, body: ReturnType<typeof html>): Response {
	return htmlResponse(
		page(
			{
				title: "Transparency",
				description:
					"Merkle-tree snapshots of the published dataset, anchored in the Root Ledger, so any quiet edit is provable.",
				path: "/transparency",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/transparency`,
			},
			body,
		),
		{ cacheSeconds: 60 },
	);
}
