import type { Ctx } from "../context";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html } from "../view/html";
import { badge, notice } from "../view/components";
import { CATALOG_SOURCE, IMMUNITY, IMMUNITY_NOTE, STATUTE_NOTE, STATUTES, authorizesLife } from "../data/catalog";
import { slug } from "./wall";

export function statutes(ctx: Ctx): Response {
	const lifeTitles = STATUTES.filter(authorizesLife);

	const body = html`
		<p class="eyebrow">Research catalog</p>
		<h1>The statutes an official is measured against</h1>
		<p class="lede">
			Elements and statutory maxima for the titles that reach abuse of office.
			Generated from <code>${CATALOG_SOURCE}</code>, the same source the backend
			serves at <code>/reckoning/statutes</code>.
		</p>

		${notice("plain", html`<strong>${STATUTE_NOTE}</strong>`)}

		<section aria-labelledby="life-h">
			<h2 id="life-h">Where the law itself authorizes life</h2>
			<p class="muted">
				${lifeTitles.length} of the cataloged titles authorize any term of years
				or life on the death-resulting and aggravated predicates. When a
				conviction is obtained on such a count, counsel seeks the statutory
				maximum the same law provides. The ceiling is the statute's; the platform
				invents nothing above it.
			</p>
			<ul class="bullets">
				${lifeTitles.map(
					(s) => html`<li>
						<a href="#${slug(s.citation)}">${s.citation}</a> — ${s.title}
					</li>`,
				)}
			</ul>
		</section>

		<div class="rule"></div>

		<section aria-labelledby="all-h">
			<h2 id="all-h">Full catalog</h2>
			${STATUTES.map(
				(s) => html`<div class="card" id="${slug(s.citation)}">
					<h3>${s.citation} — ${s.title}</h3>
					<p>
						${badge(s.kind, s.kind === "criminal" ? "criminal" : "civil")}
						${authorizesLife(s) ? badge("life authorized", "life") : ""}
						${s.finding_types.map((t) => badge(t.replace(/_/g, " ")))}
					</p>
					<h4 class="faint">Elements</h4>
					<ol class="bullets">
						${s.elements.map((e) => html`<li>${e}</li>`)}
					</ol>
					<dl class="meta">
						<dt>Maximum</dt>
						<dd>${s.statutory_maximum}</dd>
						<dt>Research note</dt>
						<dd>${s.research_note}</dd>
					</dl>
				</div>`,
			)}
		</section>

		<section aria-labelledby="imm-h">
			<h2 id="imm-h">Immunity is not the end of the inquiry</h2>
			<p class="muted">
				${IMMUNITY.length} doctrines are cataloged with the limits courts
				themselves recognize. ${IMMUNITY_NOTE}
			</p>
			<p class="actions"><a class="btn" href="/immunity">Read the immunity catalog</a></p>
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "Statute catalog",
				description:
					"Elements, statutory maxima, and research notes for 18 U.S.C. §§ 241, 242, 1512, 1621, 1622 and 42 U.S.C. § 1983.",
				path: "/statutes",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/statutes`,
			},
			body,
		),
		{ cacheSeconds: 3600 },
	);
}

export function immunity(ctx: Ctx): Response {
	const body = html`
		<p class="eyebrow">Doctrine research</p>
		<h1>Immunity doctrines and their recognized limits</h1>
		<p class="lede">
			Immunity bars certain damages claims. It does not legalize the underlying
			conduct, and it does not bar criminal prosecution, bar discipline, or
			injunctive relief. Those are the tracks this platform documents.
		</p>

		${notice("plain", html`<strong>${IMMUNITY_NOTE}</strong>`)}

		<section aria-label="Doctrines">
			${IMMUNITY.map(
				(n) => html`<div class="card" id="${slug(n.doctrine)}">
					<h3>${n.doctrine}</h3>
					<dl class="meta">
						<dt>Leading case</dt>
						<dd>${n.leading_case}</dd>
						<dt>Scope</dt>
						<dd>${n.scope}</dd>
						<dt>Recognized limits</dt>
						<dd>${n.recognized_limits}</dd>
					</dl>
				</div>`,
			)}
		</section>

		<section aria-labelledby="why-h">
			<h2 id="why-h">Why this page exists</h2>
			<p class="muted">
				A shield that blocks a damages suit is routinely presented to victims as
				though it ended the matter. It does not. Where immunity forecloses
				damages, the criminal referral, the bar complaint, the municipal
				pattern-and-practice claim, and the public record remain available — and
				this platform pursues each of them through counsel, within the law.
			</p>
			<p class="actions">
				<a class="btn" href="/statutes">Statute catalog</a>
				<a class="btn" href="/doctrine">The doctrine</a>
			</p>
		</section>
	`;

	return htmlResponse(
		page(
			{
				title: "Immunity doctrines",
				description:
					"Absolute prosecutorial immunity, qualified immunity, and judicial immunity — scope and the limits courts recognize.",
				path: "/immunity",
				config: ctx.cfg,
				canonical: `${ctx.url.origin}/immunity`,
			},
			body,
		),
		{ cacheSeconds: 3600 },
	);
}
