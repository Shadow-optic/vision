export { MyWorkflow } from "./workflow";
export { WorkflowStatusDO } from "./durable-object";

const PAGE = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>VisionInjustice</title>
  <style>
    body { font-family: ui-sans-serif, system-ui, sans-serif; max-width: 42rem; margin: 4rem auto; padding: 0 1.5rem; line-height: 1.5; color: #111; }
    h1 { font-size: 1.5rem; }
    code { background: #f3f4f6; padding: 0.1rem 0.35rem; border-radius: 4px; }
    .note { color: #4b5563; font-size: 0.95rem; }
  </style>
</head>
<body>
  <h1>VisionInjustice</h1>
  <p>Systemic criminal-justice accountability platform. The API, ledger, and engines run as the Rust service in this repository (<code>cargo run -p vi-api</code>).</p>
  <p class="note">This Worker is a public landing page so Cloudflare Workers Builds (connected to this GitHub repo) can complete. It does not serve case data and does not auto-publish findings.</p>
</body>
</html>`;

export default {
	async fetch(): Promise<Response> {
		return new Response(PAGE, {
			headers: {
				"content-type": "text/html; charset=utf-8",
				"cache-control": "public, max-age=300",
			},
		});
	},
};
