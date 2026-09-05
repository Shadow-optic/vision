import type { Ctx } from "../context";
import type { EnginesResponse, LedgerVerifyResponse, WallResponse } from "../api-types";
import { getJson } from "../upstream";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { notice, stat, unconfigured, offline } from "../view/components";
import { ENGINES } from "../data/engines";
import { STATUTES } from "../data/catalog";

export async function home(ctx: Ctx): Promise<Response> {
	const [wall, engines, ledger] = await Promise.all([
		getJson<WallResponse>(ctx.cfg, "/reckoning/wall", { ttl: 60, ctx: ctx.waitUntil }),
		getJson<EnginesResponse>(ctx.cfg, "/engines", { ttl: 60, ctx: ctx.waitUntil }),
		getJson<LedgerVerifyResponse>(ctx.cfg, "/ledger/verify", { ttl: 60, ctx: ctx.waitUntil }),
	]);

	const published = wall.state === "ok" ? wall.data.entries.length : null;
	const findings =
		wall.state === "ok"
			? wall.data.entries.reduce((n, e) => n + Number(e.substantiated_findings || 0), 0)
			: null;
	const ledgerEntries = ledger.state === "ok" ? ledger.data.entries : null;
	const ledgerOk = ledger.state === "ok" ? ledger.data.ok : null;
	const engineRows =
		engines.state === "ok"
			? engines.data.engines.reduce((n, e) => n + Number(e.rows || 0), 0)
			: null;

	const dash = "—";
	const liveNote =
		wall.state === "unconfigured"
			? unconfigured("register data")
			: wall.state === "error"
				? offline("register data", wall.detail)
				: null;

	const body = html`
		<section class="hero">
			<p class="eyebrow">Individual Accountability Doctrine</p>
			<h1>Officials who abused the law are held to the law.</h1>
			<p class="lede">
				VisionInjustice documents abuse of office by named individuals using
				public records only. Licensed counsel substantiates every finding before
				it is published. The platform files no charges — it builds the record,
				names the individual, and refers the case to the authorities that can.
			</p>
			<p class="doctrine">
				Not the office. The person. Not a settlement in the dark. An admission on
				the record. Where the same criminal law authorizes life, counsel seeks
				life.
			</p>
			<div class="actions">
				<a class="btn btn-primary" href="/wall">Open the Wall of Injustice</a>
				<a class="btn" href="/doctrine">Read the doctrine</a>
				<a class="btn" href="/api">Public JSON API</a>
			</div>
		</section>

		${liveNote ? html`<section>${liveNote}</section>` : ""}

		<section aria-labelledby="numbers">
			<h2 id="numbers">Where the record stands</h2>
			<div class="grid four">
				${stat(
					"Officials published",
					published === null ? dash : published,
					"counsel-substantiated",
				)}
				${stat("Substantiated findings", findings === null ? dash : findings, "public records")}
				${stat(
					"Ledger entries",
					ledgerEntries === null ? dash : ledgerEntries,
					ledgerOk === null ? "chain status unknown" : ledgerOk ? "chain verified" : "chain broken",
				)}
				${stat("Records under analysis", engineRows === null ? dash : engineRows, "across 14 engines")}
			</div>
		</section>

		<section aria-labelledby="how">
			<h2 id="how">How a name reaches the wall</h2>
			<div class="grid two">
				<div class="card">
					<h3>1 · Public records in</h3>
					<p class="muted">
						Opinions, dockets, disciplinary records, and published findings are
						ingested from public sources. No OSINT, no leaks, no purchased data,
						no private contact information.
					</p>
				</div>
				<div class="card">
					<h3>2 · Individual resolution</h3>
					<p class="muted">
						The Reckoning engine resolves conduct to a named person — prosecutor,
						officer, judge, or expert — by public identifiers: name, office, bar
						number, badge number.
					</p>
				</div>
				<div class="card">
					<h3>3 · Counsel substantiation</h3>
					<p class="muted">
						An automated flag publishes nothing, ever. A licensed attorney must
						mark the finding <code>substantiated</code> against the underlying
						public record. That review is the publication gate.
					</p>
				</div>
				<div class="card">
					<h3>4 · Publication and referral</h3>
					<p class="muted">
						Once substantiated, the official-conduct record publishes: name,
						office, bar or badge, citation, finding. Counsel-reviewed criminal
						referrals, bar complaints, and § 1983 scaffolds go to the bodies with
						authority to act.
					</p>
				</div>
			</div>
		</section>

		<section aria-labelledby="law">
			<h2 id="law">The same law, applied back</h2>
			<p class="muted">
				${STATUTES.length} titles are cataloged with their elements and statutory
				maxima — including the color-of-law provisions that authorize any term of
				years or life when death results. Counsel advocates the maximum the
				statute provides. Nothing beyond it.
			</p>
			<div class="actions">
				<a class="btn" href="/statutes">Statute catalog</a>
				<a class="btn" href="/immunity">Immunity doctrines and their limits</a>
			</div>
		</section>

		<section aria-labelledby="engines">
			<h2 id="engines">${ENGINES.length} engines, one record</h2>
			<p class="muted">
				Every claim on this site traces to an engine, a query, and a ledger entry.
				<a href="/engines">See the engines and their live row counts</a>.
			</p>
			${notice(
				"gate",
				html`<strong>Guardrail.</strong> Pending automated flags are invisible to
					the public. Counsel may place a hold on a published card for victim
					privacy or a pending correction — a hold is a suppression, never a
					second opt-in for the official.`,
			)}
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "Individual accountability",
				description:
					"Public-record accountability for public officials who abused their office. Counsel-substantiated findings, criminal referrals, and the Wall of Injustice.",
				path: "/",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/`,
			},
			body,
		),
		{ cacheSeconds: 60 },
	);
}
