/**
 * Single stylesheet, served from `/assets/app.<hash>.css`. No inline `style`
 * attributes anywhere in the app, so the CSP can forbid inline styles outright
 * — hence the pre-computed meter widths at the bottom of this file.
 */
const BASE = `
:root {
  --bg: #0b0d10;
  --bg-elev: #12161b;
  --bg-elev-2: #171d24;
  --line: #263140;
  --line-soft: #1c242e;
  --text: #e8edf3;
  --text-dim: #a3b0c0;
  --text-faint: #78889b;
  --accent: #c8102e;
  --accent-soft: #3a1218;
  --gold: #d8b169;
  --ok: #3fb27f;
  --warn: #d9a13b;
  --radius: 10px;
  --mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  --sans: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  --serif: ui-serif, Georgia, Cambria, "Times New Roman", serif;
}

*, *::before, *::after { box-sizing: border-box; }

html { -webkit-text-size-adjust: 100%; }

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: var(--sans);
  font-size: 16px;
  line-height: 1.6;
}

a { color: #7fb5ff; text-decoration-thickness: 1px; text-underline-offset: 2px; }
a:hover { color: #a8cdff; }

:focus-visible {
  outline: 2px solid var(--gold);
  outline-offset: 2px;
  border-radius: 4px;
}

.skip {
  position: absolute;
  left: -9999px;
  top: 0;
  background: var(--gold);
  color: #14100a;
  padding: 0.6rem 1rem;
  z-index: 20;
  font-weight: 600;
}
.skip:focus { left: 0.5rem; top: 0.5rem; }

.wrap { width: 100%; max-width: 68rem; margin: 0 auto; padding: 0 1.25rem; }

/* ---------- header ---------- */

.site-header {
  border-bottom: 1px solid var(--line);
  background: linear-gradient(180deg, #0e1216, #0b0d10);
  position: sticky;
  top: 0;
  z-index: 10;
  backdrop-filter: blur(6px);
}
.header-row {
  display: flex;
  align-items: center;
  gap: 1rem;
  min-height: 4rem;
  flex-wrap: wrap;
}
.brand {
  display: inline-flex;
  align-items: center;
  gap: 0.6rem;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--text);
  text-decoration: none;
  font-size: 1.05rem;
}
.brand .mark {
  width: 1.6rem; height: 1.6rem;
  display: inline-block; flex: none;
}
.brand small {
  display: block;
  font-weight: 500;
  font-size: 0.7rem;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--text-faint);
}
nav.primary { margin-left: auto; }
nav.primary ul {
  display: flex; gap: 0.15rem; list-style: none; margin: 0; padding: 0; flex-wrap: wrap;
}
nav.primary a {
  display: block;
  padding: 0.4rem 0.65rem;
  border-radius: 7px;
  color: var(--text-dim);
  text-decoration: none;
  font-size: 0.92rem;
}
nav.primary a:hover { background: var(--bg-elev-2); color: var(--text); }
nav.primary a[aria-current="page"] { background: var(--accent-soft); color: #ffd8de; }

/* ---------- layout blocks ---------- */

main { padding: 2.25rem 0 4rem; }
section + section { margin-top: 2.5rem; }

h1 { font-size: clamp(1.65rem, 1.2rem + 1.6vw, 2.4rem); line-height: 1.2; margin: 0 0 0.6rem; letter-spacing: -0.01em; }
h2 { font-size: 1.3rem; margin: 0 0 0.75rem; letter-spacing: -0.01em; }
h3 { font-size: 1.02rem; margin: 0 0 0.4rem; }
p { margin: 0 0 1rem; }
.lede { font-size: 1.1rem; color: var(--text-dim); max-width: 46rem; }
.eyebrow {
  font-family: var(--mono);
  font-size: 0.72rem;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--accent);
  margin: 0 0 0.5rem;
}
.muted { color: var(--text-dim); }
.faint { color: var(--text-faint); font-size: 0.88rem; }
.center { text-align: center; }

.hero {
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background:
    radial-gradient(1200px 320px at 12% -30%, rgba(200,16,46,0.16), transparent),
    var(--bg-elev);
  padding: 2rem 1.75rem;
}
.hero .doctrine {
  font-family: var(--serif);
  font-size: 1.05rem;
  color: var(--gold);
  border-left: 3px solid var(--accent);
  padding-left: 1rem;
  margin: 1.25rem 0 0;
  max-width: 44rem;
}

.actions { display: flex; gap: 0.65rem; flex-wrap: wrap; margin-top: 1.5rem; }
.btn {
  display: inline-block;
  padding: 0.6rem 1.1rem;
  border-radius: 8px;
  border: 1px solid var(--line);
  background: var(--bg-elev-2);
  color: var(--text);
  text-decoration: none;
  font-weight: 600;
  font-size: 0.95rem;
  cursor: pointer;
}
.btn:hover { border-color: #38485c; color: var(--text); }
.btn-primary { background: var(--accent); border-color: var(--accent); color: #fff; }
.btn-primary:hover { background: #a90d26; color: #fff; }

.grid { display: grid; gap: 1rem; }
.grid.two { grid-template-columns: repeat(auto-fit, minmax(19rem, 1fr)); }
.grid.three { grid-template-columns: repeat(auto-fit, minmax(15rem, 1fr)); }
.grid.four { grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr)); }

.card {
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--bg-elev);
  padding: 1.15rem 1.25rem;
}
.card h3 a { text-decoration: none; }
.card .kv { font-family: var(--mono); font-size: 0.8rem; color: var(--text-faint); }
.card + .card { margin-top: 0.9rem; }
.grid .card + .card { margin-top: 0; }
.card h4 { margin: 1rem 0 0.35rem; font-size: 0.78rem; letter-spacing: 0.08em; text-transform: uppercase; }
.card ol.bullets { margin: 0 0 0.85rem 1.15rem; padding: 0; color: var(--text-dim); }
.card ol.bullets li { margin-bottom: 0.25rem; }

.stat { border: 1px solid var(--line); border-radius: var(--radius); background: var(--bg-elev); padding: 1rem 1.15rem; }
.stat .n { font-size: 1.75rem; font-weight: 700; font-variant-numeric: tabular-nums; line-height: 1.1; }
.stat .l { font-size: 0.78rem; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-faint); }

.badge {
  display: inline-block;
  font-family: var(--mono);
  font-size: 0.72rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  padding: 0.16rem 0.5rem;
  border-radius: 999px;
  border: 1px solid var(--line);
  color: var(--text-dim);
  white-space: nowrap;
}
.badge-substantiated { border-color: #2f5f49; background: #10241c; color: #8fe0bb; }
.badge-referred { border-color: #6a2530; background: #241014; color: #ffb3bf; }
.badge-criminal { border-color: #6a2530; background: #241014; color: #ffb3bf; }
.badge-civil { border-color: #2c4a6b; background: #101a24; color: #a9cdf5; }
.badge-life { border-color: #7a5a20; background: #241c0f; color: var(--gold); }
/* Something read in part, not in whole: an opinion extract rather than the
   court's whole text, or a feed stopped part-way through a list. */
.badge-partial { border-color: #6a5320; background: #241f0f; color: #e8cd8a; }

.notice {
  border: 1px solid var(--line);
  border-left: 3px solid var(--gold);
  border-radius: 8px;
  background: var(--bg-elev-2);
  padding: 0.9rem 1.1rem;
  color: var(--text-dim);
  font-size: 0.94rem;
}
.notice strong { color: var(--text); }
.notice-offline { border-left-color: var(--warn); }
.notice-gate { border-left-color: var(--ok); }

.rule { height: 1px; background: var(--line-soft); border: 0; margin: 2rem 0; }

table.data {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.94rem;
}
table.data caption { text-align: left; color: var(--text-faint); font-size: 0.85rem; padding-bottom: 0.5rem; }
table.data th, table.data td {
  text-align: left;
  padding: 0.6rem 0.7rem;
  border-bottom: 1px solid var(--line-soft);
  vertical-align: top;
}
table.data th { color: var(--text-faint); font-size: 0.76rem; letter-spacing: 0.08em; text-transform: uppercase; font-weight: 600; }
table.data tbody tr:hover { background: #111820; }
.num { font-variant-numeric: tabular-nums; text-align: right; }

.table-scroll { overflow-x: auto; border: 1px solid var(--line); border-radius: var(--radius); background: var(--bg-elev); }
.table-scroll > table.data th:first-child, .table-scroll > table.data td:first-child { padding-left: 1.1rem; }

form.filters {
  display: flex; gap: 0.6rem; flex-wrap: wrap; align-items: flex-end;
  border: 1px solid var(--line); border-radius: var(--radius);
  background: var(--bg-elev); padding: 1rem 1.15rem; margin-bottom: 1.25rem;
}
.field { display: flex; flex-direction: column; gap: 0.3rem; min-width: 12rem; flex: 1 1 12rem; }
label { font-size: 0.78rem; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-faint); font-weight: 600; }
input[type="search"], input[type="text"], select {
  background: #0d1116;
  color: var(--text);
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0.5rem 0.6rem;
  font: inherit;
  font-size: 0.95rem;
  width: 100%;
}
select { appearance: none; background-image: linear-gradient(45deg, transparent 50%, var(--text-faint) 50%), linear-gradient(135deg, var(--text-faint) 50%, transparent 50%); background-position: calc(100% - 18px) 55%, calc(100% - 13px) 55%; background-size: 5px 5px, 5px 5px; background-repeat: no-repeat; padding-right: 2rem; }

.score {
  display: inline-flex; align-items: baseline; gap: 0.3rem;
  font-family: var(--mono);
}
.score b { font-size: 1.15rem; }
.meter { height: 6px; border-radius: 999px; background: #1b2330; overflow: hidden; margin-top: 0.35rem; }
.meter > span { display: block; height: 100%; background: linear-gradient(90deg, var(--warn), var(--accent)); }

ol.findings, ul.plain { list-style: none; margin: 0; padding: 0; }
ol.findings > li {
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--bg-elev);
  padding: 1rem 1.15rem;
}
ol.findings > li + li { margin-top: 0.75rem; }
ul.bullets { margin: 0 0 1rem 1.15rem; padding: 0; color: var(--text-dim); }
ul.bullets li { margin-bottom: 0.35rem; }

dl.meta { display: grid; grid-template-columns: auto 1fr; gap: 0.35rem 1rem; margin: 0; font-size: 0.93rem; }
dl.meta dt { color: var(--text-faint); font-size: 0.78rem; letter-spacing: 0.07em; text-transform: uppercase; padding-top: 0.15rem; }
dl.meta dd { margin: 0; }

code, .mono { font-family: var(--mono); font-size: 0.88em; }
code { background: #0d1116; border: 1px solid var(--line-soft); border-radius: 5px; padding: 0.08rem 0.32rem; }
pre {
  background: #0d1116; border: 1px solid var(--line); border-radius: var(--radius);
  padding: 1rem 1.1rem; overflow-x: auto; font-family: var(--mono); font-size: 0.84rem; line-height: 1.55;
}

.empty {
  border: 1px dashed var(--line);
  border-radius: var(--radius);
  padding: 2.25rem 1.5rem;
  text-align: center;
  color: var(--text-dim);
  background: #0e1216;
}
.empty h3 { color: var(--text); }

.breadcrumb { font-size: 0.85rem; color: var(--text-faint); margin-bottom: 0.75rem; }
.breadcrumb a { color: var(--text-dim); }

.site-footer {
  border-top: 1px solid var(--line);
  padding: 2rem 0 3rem;
  color: var(--text-faint);
  font-size: 0.88rem;
  background: #090b0e;
}
.site-footer nav ul { display: flex; flex-wrap: wrap; gap: 0.35rem 1.1rem; list-style: none; margin: 0 0 1rem; padding: 0; }
.site-footer a { color: var(--text-dim); }
.site-footer .legal { max-width: 52rem; }

@media (max-width: 40rem) {
  .hero { padding: 1.5rem 1.15rem; }
  nav.primary { margin-left: 0; width: 100%; }
  nav.primary ul { gap: 0; }
  .header-row { padding-bottom: 0.5rem; }
}

@media (prefers-reduced-motion: reduce) {
  * { animation: none !important; transition: none !important; }
}

@media print {
  body { background: #fff; color: #000; }
  .site-header, .site-footer, .actions, form.filters { display: none; }
  .card, .hero, ol.findings > li { border-color: #999; background: #fff; }
}
`;

const METER_WIDTHS = Array.from(
	{ length: 21 },
	(_, i) => `.meter > span.w${i * 5} { width: ${i * 5}%; }`,
).join("\n");

export const CSS = `${BASE}\n${METER_WIDTHS}\n`;
