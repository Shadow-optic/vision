import type { Ctx } from "../context";
import type {
	AbuseScore,
	TrackerResponse,
	WallEntry,
	WallProfileResponse,
	WallResponse,
} from "../api-types";
import { getJson } from "../upstream";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html, raw, type Fragment } from "../view/html";
import {
	badge,
	breadcrumb,
	empty,
	notice,
	offline,
	scoreMeter,
	stat,
	unconfigured,
} from "../view/components";
import { notFoundPage } from "./errors";
import { STATUTES, authorizesLife } from "../data/catalog";

const ROLE_LABELS: Record<string, string> = {
	prosecutor: "Prosecutor",
	officer: "Officer",
	judge: "Judge",
	expert: "Expert witness",
	other: "Other official",
};

function roleLabel(role: string): string {
	return ROLE_LABELS[role] ?? role;
}

const FINDING_LABELS: Record<string, string> = {
	brady: "Brady suppression",
	giglio: "Giglio disclosure",
	batson: "Batson strike",
	discovery: "Discovery violation",
	due_process: "Due-process violation",
	witness_subornation: "Witness subornation",
};

function findingLabel(kind: string): string {
	return FINDING_LABELS[kind] ?? kind.replace(/_/g, " ");
}

const GATE = notice(
	"gate",
	html`<strong>Publication gate.</strong> Every card below rests on a
		public-record finding that a licensed attorney marked
		<code>substantiated</code>. Automated flags never appear here. This is a
		record of official conduct, not a charging document — and not a conviction.`,
);

export async function wallIndex(ctx: Ctx): Promise<Response> {
	const wall = await getJson<WallResponse>(ctx.cfg, "/reckoning/wall", {
		ttl: 60,
		ctx: ctx.waitUntil,
	});

	const q = (ctx.url.searchParams.get("q") ?? "").trim();
	const role = (ctx.url.searchParams.get("role") ?? "").trim();
	const jurisdiction = (ctx.url.searchParams.get("jurisdiction") ?? "").trim();

	let body: Fragment;

	if (wall.state === "unconfigured") {
		body = html`${header(null)}${GATE}
		<div class="rule"></div>
		${unconfigured("register data")}`;
	} else if (wall.state === "error" || wall.state === "missing") {
		body = html`${header(null)}${GATE}
		<div class="rule"></div>
		${offline("register data", wall.state === "error" ? wall.detail : "register not found")}`;
	} else {
		const all = wall.data.entries ?? [];
		const jurisdictions = [...new Set(all.map((e) => e.jurisdiction).filter(Boolean))].sort();
		const roles = [...new Set(all.map((e) => e.role).filter(Boolean))].sort();
		const needle = q.toLowerCase();
		const shown = all.filter((e) => {
			if (role && e.role !== role) return false;
			if (jurisdiction && e.jurisdiction !== jurisdiction) return false;
			if (!needle) return true;
			return [e.display_name, e.office ?? "", e.jurisdiction, e.bar_number ?? "", e.badge_number ?? ""]
				.join(" ")
				.toLowerCase()
				.includes(needle);
		});
		const referred = all.filter((e) => e.status === "referred").length;
		const findings = all.reduce((n, e) => n + Number(e.substantiated_findings || 0), 0);

		body = html`
			${header(all.length)} ${GATE}
			<section aria-label="Register totals">
				<div class="grid three">
					${stat("Officials published", all.length)} ${stat("Substantiated findings", findings)}
					${stat("Referred to authorities", referred)}
				</div>
			</section>
			<section aria-labelledby="filters-h">
				<h2 id="filters-h" class="faint">Filter the register</h2>
				<form class="filters" method="get" action="/wall" data-autosubmit>
					<div class="field">
						<label for="q">Name, office, bar or badge</label>
						<input
							id="q"
							name="q"
							type="search"
							value="${q}"
							placeholder="e.g. district attorney"
							data-filter-for="wall-table"
							data-filter-status="wall-count"
							autocomplete="off"
						/>
					</div>
					<div class="field">
						<label for="role">Role</label>
						<select id="role" name="role">
							<option value="">All roles</option>
							${roles.map(
								(r) =>
									html`<option value="${r}" ${raw(r === role ? "selected" : "")}>
										${roleLabel(r)}
									</option>`,
							)}
						</select>
					</div>
					<div class="field">
						<label for="jurisdiction">Jurisdiction</label>
						<select id="jurisdiction" name="jurisdiction">
							<option value="">All jurisdictions</option>
							${jurisdictions.map(
								(j) =>
									html`<option value="${j}" ${raw(j === jurisdiction ? "selected" : "")}>
										${j}
									</option>`,
							)}
						</select>
					</div>
					<div class="field">
						<button class="btn" type="submit" data-filter-submit>Apply</button>
					</div>
				</form>
				<p class="faint" id="wall-count" role="status">
					${shown.length === all.length
						? `${all.length} listed`
						: `${shown.length} of ${all.length} match`}
				</p>
				${shown.length === 0 ? emptyRegister(all.length > 0) : table(shown)}
			</section>
		`;
	}

	return htmlResponse(
		page(
			{
				title: "Wall of Injustice",
				description:
					"Public Accountability Register: officials with public-record findings substantiated by licensed counsel.",
				path: "/wall",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/wall`,
				noindex: q !== "" || role !== "" || jurisdiction !== "",
			},
			body,
		),
		{ cacheSeconds: 60 },
	);
}

function header(count: number | null): Fragment {
	return html`
		<p class="eyebrow">Public Accountability Register</p>
		<h1>Wall of Injustice</h1>
		<p class="lede">
			Named officials whose conduct is documented in public records and
			substantiated by licensed counsel${count === null ? "" : ` — ${count} published`}.
			Each card links to the citation behind every finding.
		</p>
	`;
}

function emptyRegister(hasFilters: boolean): Fragment {
	return hasFilters
		? empty(
				"No official matches those filters",
				html`<p>Clear the filters to see the full register.</p>
					<p class="actions center"><a class="btn" href="/wall">Show all</a></p>`,
			)
		: empty(
				"The register is empty",
				html`<p>
						No public-record finding has cleared counsel review yet. That is the
						gate working as designed: automated flags never publish, and a name
						only appears once an attorney substantiates the underlying record.
					</p>
					<p class="actions center">
						<a class="btn" href="/doctrine">How review works</a>
					</p>`,
			);
}

function table(entries: WallEntry[]): Fragment {
	return html`<div class="table-scroll">
		<table class="data" id="wall-table">
			<caption>
				Officials with counsel-substantiated public-record findings.
			</caption>
			<thead>
				<tr>
					<th scope="col">Official</th>
					<th scope="col">Role</th>
					<th scope="col">Office</th>
					<th scope="col">Jurisdiction</th>
					<th scope="col">Public identifiers</th>
					<th scope="col" class="num">Findings</th>
					<th scope="col">Status</th>
				</tr>
			</thead>
			<tbody>
				${entries.map(
					(e) => html`<tr>
						<th scope="row">
							<a href="/wall/${e.actor_id}">${e.display_name}</a>
						</th>
						<td>${roleLabel(e.role)}</td>
						<td>${e.office ?? "—"}</td>
						<td>${e.jurisdiction}</td>
						<td class="mono faint">
							${e.bar_number ? `Bar ${e.bar_number}` : ""}
							${e.bar_number && e.badge_number ? " · " : ""}
							${e.badge_number ? `Badge ${e.badge_number}` : ""}
							${!e.bar_number && !e.badge_number ? "—" : ""}
						</td>
						<td class="num">${e.substantiated_findings}</td>
						<td>${badge(e.status, e.status === "referred" ? "referred" : "substantiated")}</td>
					</tr>`,
				)}
			</tbody>
		</table>
	</div>`;
}

export async function wallProfile(ctx: Ctx, actorId: string): Promise<Response> {
	const profile = await getJson<WallProfileResponse>(
		ctx.cfg,
		`/reckoning/wall/${encodeURIComponent(actorId)}`,
		{ ttl: 60, ctx: ctx.waitUntil },
	);

	if (profile.state === "unconfigured") {
		return htmlResponse(
			page(
				{
					title: "Official record",
					description: "Public Accountability Register entry.",
					path: "/wall",
					config: ctx.cfg,
					noindex: true,
				},
				html`${breadcrumb([{ href: "/wall", label: "Wall of Injustice" }, { label: "Record" }])}
					<h1>Official record</h1>
					${unconfigured("register data")}`,
			),
			{ status: 503, cacheSeconds: 0 },
		);
	}

	if (profile.state === "missing") {
		return notFoundPage(
			ctx,
			"No published record for that identifier",
			html`<p>
					Either no such official is on the register, or counsel has placed a hold
					on the card for victim privacy or a pending correction. A hold suppresses
					publication; it does not erase the underlying public record or the
					referral.
				</p>
				<p class="actions center"><a class="btn" href="/wall">Back to the register</a></p>`,
		);
	}

	if (profile.state === "error") {
		return htmlResponse(
			page(
				{
					title: "Official record",
					description: "Public Accountability Register entry.",
					path: "/wall",
					config: ctx.cfg,
					noindex: true,
				},
				html`${breadcrumb([{ href: "/wall", label: "Wall of Injustice" }, { label: "Record" }])}
					<h1>Official record</h1>
					${offline("this record", profile.detail)}`,
			),
			{ status: 502, cacheSeconds: 0 },
		);
	}

	const entry = profile.data.entry;
	const [score, tracker] = await Promise.all([
		getJson<AbuseScore>(ctx.cfg, `/reckoning/actors/${encodeURIComponent(actorId)}/score`, {
			ttl: 60,
			ctx: ctx.waitUntil,
		}),
		getJson<TrackerResponse>(ctx.cfg, "/reckoning/tracker", { ttl: 60, ctx: ctx.waitUntil }),
	]);

	const packages =
		tracker.state === "ok"
			? tracker.data.packages.filter((p) => p.actor_id === entry.actor_id)
			: [];
	const records = Array.isArray(entry.public_records) ? entry.public_records : [];
	const findingTypes = [...new Set(records.map((r) => r.finding_type))];
	const applicable = STATUTES.filter((s) =>
		s.finding_types.some((t) => findingTypes.includes(t)),
	);

	const body = html`
		${breadcrumb([
			{ href: "/wall", label: "Wall of Injustice" },
			{ label: entry.display_name },
		])}
		<p class="eyebrow">${roleLabel(entry.role)} · ${entry.jurisdiction}</p>
		<h1>${entry.display_name}</h1>
		<p class="lede">
			${entry.office ? `${entry.office}. ` : ""}${entry.substantiated_findings}
			public-record finding${entry.substantiated_findings === 1 ? "" : "s"}
			substantiated by licensed counsel.
		</p>

		<section class="grid two">
			<div class="card">
				<h3>Public-record identity</h3>
				<dl class="meta">
					<dt>Role</dt>
					<dd>${roleLabel(entry.role)}</dd>
					<dt>Office</dt>
					<dd>${entry.office ?? "Not recorded"}</dd>
					<dt>Jurisdiction</dt>
					<dd>${entry.jurisdiction}</dd>
					<dt>Bar no.</dt>
					<dd class="mono">${entry.bar_number ?? "—"}</dd>
					<dt>Badge no.</dt>
					<dd class="mono">${entry.badge_number ?? "—"}</dd>
					<dt>Status</dt>
					<dd>${badge(entry.status, entry.status === "referred" ? "referred" : "substantiated")}</dd>
				</dl>
				<p class="faint">
					Public identifiers only. No photograph, home address, family
					information, or private contact data is collected or published.
				</p>
			</div>
			<div class="card">
				<h3>Abuse score</h3>
				${score.state === "ok"
					? html`${scoreMeter(score.data.score)}
						<dl class="meta">
							<dt>Findings</dt>
							<dd>${score.data.substantiated_findings} substantiated</dd>
							<dt>Flags</dt>
							<dd>${score.data.substantiated_flags} substantiated</dd>
							<dt>Sources</dt>
							<dd>${score.data.corroboration_sources} corroborating</dd>
							<dt>Recent</dt>
							<dd>${score.data.recent_5yr} within five years</dd>
						</dl>
						<p class="faint mono">${score.data.formula}</p>
						<p class="faint">
							A prioritization signal, not a verdict. Pending flags contribute
							nothing.
						</p>`
					: html`<p class="faint">Score unavailable right now.</p>`}
			</div>
		</section>

		<section aria-labelledby="records-h">
			<h2 id="records-h">Substantiated public-record findings</h2>
			${records.length === 0
				? empty(
						"No finding detail published",
						html`<p>The card is published without per-finding detail.</p>`,
					)
				: html`<ol class="findings">
						${records.map(
							(r) => html`<li>
								<h3>${findingLabel(r.finding_type)}</h3>
								<dl class="meta">
									<dt>Citation</dt>
									<dd>${r.citation ?? "Not recorded"}</dd>
									<dt>Date</dt>
									<dd>${r.finding_date ?? "Not recorded"}</dd>
									${r.source_url
										? html`<dt>Source</dt>
											<dd>
												<a href="${r.source_url}" rel="nofollow noopener ugc">Public record</a>
											</dd>`
										: ""}
								</dl>
								${r.summary ? html`<p class="muted">${r.summary}</p>` : ""}
							</li>`,
						)}
					</ol>`}
		</section>

		${applicable.length > 0
			? html`<section aria-labelledby="statutes-h">
					<h2 id="statutes-h">Titles counsel researches for this conduct</h2>
					<p class="muted">
						Mapped from the finding types above. A mapping is a research pointer,
						not an accusation that the elements are met — willfulness and
						agreement are for counsel and a fact-finder.
					</p>
					<div class="grid two">
						${applicable.map(
							(s) => html`<div class="card">
								<h3><a href="/statutes#${slug(s.citation)}">${s.citation}</a></h3>
								<p class="muted">${s.title}</p>
								<p>
									${badge(s.kind, s.kind === "criminal" ? "criminal" : "civil")}
									${authorizesLife(s) ? badge("life authorized", "life") : ""}
								</p>
								<p class="faint">${s.statutory_maximum}</p>
							</div>`,
						)}
					</div>
				</section>`
			: ""}

		<section aria-labelledby="packages-h">
			<h2 id="packages-h">Legal action</h2>
			${packages.length === 0
				? html`<p class="muted">
						No attorney-reviewed package has been referred for this official yet.
					</p>`
				: html`<div class="table-scroll">
						<table class="data">
							<thead>
								<tr>
									<th scope="col">Package</th>
									<th scope="col">Kind</th>
									<th scope="col">Status</th>
								</tr>
							</thead>
							<tbody>
								${packages.map(
									(p) => html`<tr>
										<th scope="row" class="mono faint">${p.package_id}</th>
										<td>${p.action_kind.replace(/_/g, " ")}</td>
										<td>${badge(p.status, p.status === "referred" ? "referred" : "substantiated")}</td>
									</tr>`,
								)}
							</tbody>
						</table>
					</div>`}
			${notice(
				"plain",
				html`<strong>Due process.</strong> ${entry.display_name} is presumed
					innocent of any criminal charge. Nothing here is a charge, a conviction,
					or legal advice. Factual corrections and privacy holds are handled by
					counsel — <a href="/corrections">see the correction process</a>.`,
			)}
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: entry.display_name,
				description: `${entry.display_name} — ${roleLabel(entry.role)}, ${entry.jurisdiction}. ${entry.substantiated_findings} counsel-substantiated public-record findings.`,
				path: `/wall/${entry.actor_id}`,
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/wall/${entry.actor_id}`,
			},
			body,
		),
		{ cacheSeconds: 60 },
	);
}

export function slug(citation: string): string {
	return citation
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-|-$/g, "");
}
