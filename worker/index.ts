export { MyWorkflow } from "./workflow";
export { WorkflowStatusDO } from "./durable-object";

const STATES: { code: string; name: string; circuit: string }[] = [
	{ code: "AL", name: "Alabama", circuit: "11th Cir." },
	{ code: "AK", name: "Alaska", circuit: "9th Cir." },
	{ code: "AZ", name: "Arizona", circuit: "9th Cir." },
	{ code: "AR", name: "Arkansas", circuit: "8th Cir." },
	{ code: "CA", name: "California", circuit: "9th Cir." },
	{ code: "CO", name: "Colorado", circuit: "10th Cir." },
	{ code: "CT", name: "Connecticut", circuit: "2nd Cir." },
	{ code: "DE", name: "Delaware", circuit: "3rd Cir." },
	{ code: "FL", name: "Florida", circuit: "11th Cir." },
	{ code: "GA", name: "Georgia", circuit: "11th Cir." },
	{ code: "HI", name: "Hawaii", circuit: "9th Cir." },
	{ code: "ID", name: "Idaho", circuit: "9th Cir." },
	{ code: "IL", name: "Illinois", circuit: "7th Cir." },
	{ code: "IN", name: "Indiana", circuit: "7th Cir." },
	{ code: "IA", name: "Iowa", circuit: "8th Cir." },
	{ code: "KS", name: "Kansas", circuit: "10th Cir." },
	{ code: "KY", name: "Kentucky", circuit: "6th Cir." },
	{ code: "LA", name: "Louisiana", circuit: "5th Cir." },
	{ code: "ME", name: "Maine", circuit: "1st Cir." },
	{ code: "MD", name: "Maryland", circuit: "4th Cir." },
	{ code: "MA", name: "Massachusetts", circuit: "1st Cir." },
	{ code: "MI", name: "Michigan", circuit: "6th Cir." },
	{ code: "MN", name: "Minnesota", circuit: "8th Cir." },
	{ code: "MS", name: "Mississippi", circuit: "5th Cir." },
	{ code: "MO", name: "Missouri", circuit: "8th Cir." },
	{ code: "MT", name: "Montana", circuit: "9th Cir." },
	{ code: "NE", name: "Nebraska", circuit: "8th Cir." },
	{ code: "NV", name: "Nevada", circuit: "9th Cir." },
	{ code: "NH", name: "New Hampshire", circuit: "1st Cir." },
	{ code: "NJ", name: "New Jersey", circuit: "3rd Cir." },
	{ code: "NM", name: "New Mexico", circuit: "10th Cir." },
	{ code: "NY", name: "New York", circuit: "2nd Cir." },
	{ code: "NC", name: "North Carolina", circuit: "4th Cir." },
	{ code: "ND", name: "North Dakota", circuit: "8th Cir." },
	{ code: "OH", name: "Ohio", circuit: "6th Cir." },
	{ code: "OK", name: "Oklahoma", circuit: "10th Cir." },
	{ code: "OR", name: "Oregon", circuit: "9th Cir." },
	{ code: "PA", name: "Pennsylvania", circuit: "3rd Cir." },
	{ code: "RI", name: "Rhode Island", circuit: "1st Cir." },
	{ code: "SC", name: "South Carolina", circuit: "4th Cir." },
	{ code: "SD", name: "South Dakota", circuit: "8th Cir." },
	{ code: "TN", name: "Tennessee", circuit: "6th Cir." },
	{ code: "TX", name: "Texas", circuit: "5th Cir." },
	{ code: "UT", name: "Utah", circuit: "10th Cir." },
	{ code: "VT", name: "Vermont", circuit: "2nd Cir." },
	{ code: "VA", name: "Virginia", circuit: "4th Cir." },
	{ code: "WA", name: "Washington", circuit: "9th Cir." },
	{ code: "WV", name: "West Virginia", circuit: "4th Cir." },
	{ code: "WI", name: "Wisconsin", circuit: "7th Cir." },
	{ code: "WY", name: "Wyoming", circuit: "10th Cir." },
];

const OPTIONS = STATES.map(
	(s) => `<option value="${s.code}" data-circuit="${s.circuit}">${s.name} (${s.code})</option>`,
).join("");

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
    label { display: block; margin: 1.25rem 0 0.35rem; font-weight: 600; }
    select { width: 100%; padding: 0.45rem 0.5rem; }
  </style>
</head>
<body>
  <h1>VisionInjustice</h1>
  <p>Systemic criminal-justice accountability platform. The API, ledger, and engines run as the Rust service in this repository (<code>cargo run -p vi-api</code>).</p>
  <p class="note">This Worker is a public landing page so Cloudflare Workers Builds (connected to this GitHub repo) can complete. It does not serve case data and does not auto-publish findings.</p>
  <label for="jurisdiction">Jurisdiction (all 50 states)</label>
  <select id="jurisdiction" aria-label="Select a state jurisdiction">
    <option value="">Select a state…</option>
    ${OPTIONS}
  </select>
  <p class="note" id="circuit">Federal circuit: —</p>
  <script>
    const sel = document.getElementById('jurisdiction');
    const out = document.getElementById('circuit');
    sel.addEventListener('change', () => {
      const opt = sel.selectedOptions[0];
      const cir = opt && opt.dataset.circuit;
      out.textContent = cir ? ('Federal circuit: ' + cir + ' — use GET /constitution/jurisdictions and POST /constitution/resolve on vi-api') : 'Federal circuit: —';
    });
  </script>
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
