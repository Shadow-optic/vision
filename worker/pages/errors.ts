import type { Ctx } from "../context";
import { htmlResponse } from "../http";
import { page } from "../view/layout";
import { html, type Fragment } from "../view/html";
import { empty } from "../view/components";
import type { SiteConfig } from "../env";

export function notFoundPage(ctx: Ctx, title: string, body: Fragment): Response {
	return htmlResponse(
		page(
			{
				title,
				description: "Page not found.",
				path: ctx.url.pathname,
				config: ctx.cfg,
				noindex: true,
			},
			html`<h1>${title}</h1>
				${empty(title, body)}`,
		),
		{ status: 404, cacheSeconds: 0 },
	);
}

export function notFound(ctx: Ctx): Response {
	return notFoundPage(
		ctx,
		"Page not found",
		html`<p>That page does not exist on this platform.</p>
			<p class="actions center">
				<a class="btn btn-primary" href="/">Home</a>
				<a class="btn" href="/wall">Wall of Injustice</a>
				<a class="btn" href="/api">API</a>
			</p>`,
	);
}

export function methodNotAllowed(ctx: Ctx): Response {
	const res = htmlResponse(
		page(
			{
				title: "Method not allowed",
				description: "This platform is read-only over HTTP.",
				path: ctx.url.pathname,
				config: ctx.cfg,
				noindex: true,
			},
			html`<h1>Method not allowed</h1>
				${empty(
					"Read-only",
					html`<p>
						The public platform serves <code>GET</code> and <code>HEAD</code> only.
						Publication, holds, and package generation are counsel operations on
						the backend and are not reachable from here.
					</p>`,
				)}`,
		),
		{ status: 405, cacheSeconds: 0 },
	);
	res.headers.set("allow", "GET, HEAD");
	return res;
}

export function serverError(cfg: SiteConfig, path: string, requestId: string): Response {
	return htmlResponse(
		page(
			{
				title: "Something failed",
				description: "An unexpected error occurred.",
				path,
				config: cfg,
				noindex: true,
			},
			html`<h1>Something failed on our side</h1>
				${empty(
					"Unexpected error",
					html`<p>
							The request could not be completed. Nothing was published,
							changed, or removed by this failure.
						</p>
						<p class="faint mono">Reference: ${requestId}</p>
						<p class="actions center"><a class="btn" href="/">Home</a></p>`,
				)}`,
		),
		{ status: 500, cacheSeconds: 0 },
	);
}
