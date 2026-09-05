import type { Ctx } from "../context";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { notice } from "../view/components";

export function doctrine(ctx: Ctx): Response {
	const body = html`
		<p class="eyebrow">No Bars Held</p>
		<h1>The Individual Accountability Doctrine</h1>
		<p class="lede">
			Systemic findings change policy. They do not touch the person who withheld
			the exculpatory file, swore to the false affidavit, or signed the order they
			knew was void. This platform exists to name that person from the public
			record and to move the record to the authorities that can act on it.
		</p>

		<section aria-labelledby="p-h">
			<h2 id="p-h">Six principles</h2>
			<div class="grid two">
				<div class="card">
					<h3>1 · The individual, not the office</h3>
					<p class="muted">
						Conduct is resolved to a named person by public identifiers — name,
						office, bar number, badge number. "The district attorney's office"
						is not an answer to what an individual did.
					</p>
				</div>
				<div class="card">
					<h3>2 · The fullest extent of the law</h3>
					<p class="muted">
						Every charge and every sentence the evidence supports, up to the
						statutory maximum — including life imprisonment where 18 U.S.C.
						§§ 241, 242, or 1512 authorize it on a death-resulting count. The
						ceiling belongs to the statute. Counsel argues to it, never past it.
					</p>
				</div>
				<div class="card">
					<h3>3 · Immutability</h3>
					<p class="muted">
						Every ingest, flag, review, score, package, and publication decision
						is appended to the hash-chained Root Ledger. The chain is publicly
						verifiable, so the record of what this platform did cannot be
						rewritten later — by anyone, including us.
					</p>
				</div>
				<div class="card">
					<h3>4 · Radical transparency</h3>
					<p class="muted">
						Substantiated findings, referrals, and outcomes are public, with
						victim privacy protected absolutely. The same JSON that renders these
						pages is open at <a href="/api">/api</a>.
					</p>
				</div>
				<div class="card">
					<h3>5 · No mercy for the corrupt</h3>
					<p class="muted">
						No plea that avoids an admission of wrongdoing. No settlement gagged
						by a non-disclosure agreement. No quiet resignation in place of bar
						discipline. No immunity from consequence where the law provides a
						consequence.
					</p>
				</div>
				<div class="card">
					<h3>6 · Due process for all</h3>
					<p class="muted">
						Including the accused official. Presumption of innocence, notice,
						counsel, and a factual-correction channel. The abuse being documented
						is precisely the abandonment of these protections; abandoning them in
						return would forfeit the case.
					</p>
				</div>
			</div>
		</section>

		<div class="rule"></div>

		<section aria-labelledby="gate-h">
			<h2 id="gate-h">The publication gate, exactly</h2>
			<ol class="bullets">
				<li>
					<strong>Ingest.</strong> Public records only — opinions, dockets,
					published findings, disciplinary records. No OSINT, no leaked data, no
					purchased data, no private contact information.
				</li>
				<li>
					<strong>Detection.</strong> TrustScript rules raise flags. A flag is a
					lead for a human. Flags are invisible to the public and contribute
					nothing to any score.
				</li>
				<li>
					<strong>Counsel review.</strong> A licensed attorney examines the
					underlying record and either sets
					<code>review_status = substantiated</code> or does not. This is the only
					gate.
				</li>
				<li>
					<strong>Publication.</strong> Once substantiated, the official-conduct
					record publishes — name, office, bar or badge, citation, finding. There
					is no second approval step and no opportunity for an official to veto a
					substantiated public record.
				</li>
				<li>
					<strong>Hold.</strong> Counsel may suppress a published card for victim
					privacy or a pending correction. A hold removes the public card; it does
					not withdraw a referral or alter the ledger.
				</li>
				<li>
					<strong>Referral.</strong> Attorney work product — criminal referral
					drafts, bar complaints, § 1983 scaffolds, sentencing memoranda — goes to
					prosecutors, bar authorities, and courts. This platform files nothing
					itself.
				</li>
			</ol>
			${notice(
				"gate",
				html`<strong>What this platform will never do.</strong> It does not charge
					anyone, does not adjudicate guilt, does not publish an unsubstantiated
					flag, does not publish victim identities, and does not advocate any
					punishment outside what the cited statute authorizes.`,
			)}
		</section>

		<section aria-labelledby="settle-h">
			<h2 id="settle-h">What resolution means here</h2>
			<p class="muted">
				A cash payment from a public treasury, with no admission and a gag clause,
				is not accountability — it is a cost of doing business, paid by taxpayers
				on behalf of the individual who caused the harm. Where this platform's
				work product supports a demand, that demand is:
			</p>
			<ul class="bullets">
				<li>admission of wrongdoing on the record;</li>
				<li>referral for criminal prosecution where the evidence supports it;</li>
				<li>
					permanent bar from holding office or a license, through the bar and
					licensing processes the law provides;
				</li>
				<li>expungement and restitution for the victim;</li>
				<li>no non-disclosure of the substantiated conduct.</li>
			</ul>
			<p class="faint">
				These are litigation and advocacy positions taken by counsel, not
				outcomes this platform can impose. Only courts, juries, prosecutors, and
				licensing authorities impose outcomes.
			</p>
		</section>

		<section aria-labelledby="sent-h">
			<h2 id="sent-h">Sentencing advocacy</h2>
			<p class="muted">
				After a conviction — never before — counsel files a sentencing memorandum
				that seeks the statutory maximum the same law provides. Where the
				color-of-law titles authorize any term of years or life because death
				resulted, counsel seeks life. An official who used the law as a weapon is
				sentenced under that law like anyone else: color of law is an element of
				the offense, not a discount at sentencing.
			</p>
			<p class="actions">
				<a class="btn" href="/statutes">See the statutory maxima</a>
				<a class="btn" href="/immunity">See what immunity does not cover</a>
			</p>
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "The doctrine",
				description:
					"The Individual Accountability Doctrine: six principles, the counsel-review publication gate, and the limits this platform holds itself to.",
				path: "/doctrine",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/doctrine`,
			},
			body,
		),
		{ cacheSeconds: 3600 },
	);
}

export function corrections(ctx: Ctx): Response {
	const body = html`
		<p class="eyebrow">Due process</p>
		<h1>Corrections, privacy holds, and victim requests</h1>
		<p class="lede">
			Everything published here is drawn from a public record and was
			substantiated by licensed counsel. Records can still be wrong, superseded,
			or dangerous to a victim. There is a channel for each case, and counsel —
			not an algorithm — answers it.
		</p>

		<section class="grid two">
			<div class="card">
				<h3>Named officials</h3>
				<p class="muted">
					If a published finding misstates the record — wrong person, vacated
					finding, reversed on appeal, corrected citation — send the citation and
					the controlling document. Counsel re-reviews the underlying record. If
					the record does not support the finding, the card comes down and the
					correction is appended to the ledger.
				</p>
				<p class="faint">
					A card is not removed because it is unflattering. It is removed because
					the public record does not support it.
				</p>
			</div>
			<div class="card">
				<h3>Victims and their counsel</h3>
				<p class="muted">
					A victim, or their attorney, may request that a card be held or that
					identifying context be removed. Victim privacy requests are honored
					without argument. No victim name, case-identifying detail, or contact
					information is published in the first place.
				</p>
			</div>
			<div class="card">
				<h3>Journalists and researchers</h3>
				<p class="muted">
					Every page has a JSON equivalent under <a href="/api">/api</a>, and
					every claim traces to a public-record citation. Ledger verification is
					open at <a href="/ledger">/ledger</a>.
				</p>
			</div>
			<div class="card">
				<h3>What a hold does</h3>
				<p class="muted">
					A hold suppresses the public card. It does not delete the underlying
					public record, withdraw a referral already sent to an authority, or
					rewrite the hash-chained ledger — the ledger records that the hold was
					placed, by whom, and when.
				</p>
			</div>
		</section>

		<section aria-labelledby="how-h">
			<h2 id="how-h">How to reach counsel</h2>
			<p>
				Email <a href="mailto:${ctx.cfg.contactEmail}">${ctx.cfg.contactEmail}</a>
				with the official's name as published, the citation shown on the card, and
				the document you rely on. Include a docket number where one exists.
			</p>
			${notice(
				"plain",
				html`<strong>This is not a legal-services intake.</strong> Writing to this
					address does not create an attorney-client relationship and is not a
					substitute for retaining counsel. Do not send privileged or sealed
					material.`,
			)}
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "Corrections and privacy holds",
				description:
					"How factual corrections, victim-privacy holds, and researcher requests are handled by counsel.",
				path: "/corrections",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/corrections`,
			},
			body,
		),
		{ cacheSeconds: 3600 },
	);
}
