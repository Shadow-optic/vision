# VisionInjustice

A Rust monorepo for **systemic criminal-justice accountability**. Fourteen engines operate on **public records and counsel-substantiated findings only** — no OSINT, no leaked data, no publication of pending automated flags. After licensed counsel substantiates a public-record finding, the official's public-record identity and those findings are published. The engines do not file charges. After a conviction they advocate for the statutory maximum the same law provides, including life imprisonment where 18 U.S.C. §§ 241, 242, or 1512 authorize it.

The API is an MVP that is compile-time database-URL-free (runtime-checked SQL), hash-chained, and ready to sit behind a gateway for real-world testing. It is **not** a substitute for licensed counsel, and it ships without AuthN/Z (Phase 4).

## Engines

| Engine | Crate | Status |
|---|---|---|
| Root Ledger | `vi-ledger` | BLAKE3 hash-chained, append-only, full `verify()` |
| Case-law DB | schema + `vi-api` search | Generated `tsv` + GIN; opinion search live |
| Correlation | `vi-correlation` | Pearson + Fisher CIs, odds ratios, formula-audited |
| Tactics DB | `vi-tactics` | Seeded catalog; occurrence rates from public records |
| Abuse detection | `vi-trustscript` | Lexer/parser/evaluator + rule CRUD + ledger flags |
| Zero-day sim | `vi-sim` | Seeded Monte Carlo; `POST /simulate/from-case/:id` calibrates priors from stored office stats |
| H3 intelligence | `vi-geo` | Multi-res ladder at ingest; k-ring (`grid_disk`) disparity API |
| Telemetry / ingest | `vi-ingest` | CourtListener + fixture sources, cursor checkpoints, API-triggered poll |
| JIT LASM | `vi-lasm` | Evidence package: flags, Brady leads, Monell, trial-penalty, constitution screen, ledger provenance |
| Monell atlas | `vi-monell-atlas` | Pattern-and-practice fingerprint + §1983 scaffold |
| Brady recon | `vi-brady-recon` | Expected vs disclosed evidence; gaps are *leads* |
| Trial penalty | `vi-trial-penalty` | Distributions, disparity OR, draft motion template |
| Constitution / Bill of Rights | `vi-constitution` | Native corpus (Arts. I–VII + Amends. 1–27), 50-state dropdowns, stare-decisis resolver, advisory screens |
| Reckoning / individual accountability | `vi-reckoning` | Named-actor resolution, formula-audited abuse scores, counsel-reviewed referral/bar/§1983 packages, Wall of Injustice for substantiated public-record findings |

`GET /engines` lists all fourteen with live row counts.

## Quick start

```bash
docker compose up -d db
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/visioninjustice

# pure tests — no DB required
cargo test -p vi-ledger -p vi-correlation -p vi-trustscript -p vi-sim -p vi-geo \
           -p vi-lasm -p vi-tactics -p vi-ingest -p vi-monell-atlas -p vi-brady-recon \
           -p vi-trial-penalty -p vi-constitution -p vi-reckoning

# with Postgres: applies migrations, then integration tests
cargo test -p vi-api

cargo run -p vi-api
```

Migrations run automatically on API (and ingest) startup via `sqlx::migrate!`.

### Demo against seeded data

```bash
curl -s localhost:8080/engines
curl -s localhost:8080/tactics
curl -s localhost:8080/tactics/aaaaaaaa-0000-4000-8000-000000000001/stats

# Fire abuse-detection on the seeded case (two pending-review flags)
curl -s -X POST localhost:8080/rules/run \
  -H 'content-type: application/json' \
  -d '{"case_id":"22222222-2222-2222-2222-222222222222"}'

curl -s 'localhost:8080/prosecutors/11111111-1111-1111-1111-111111111111/stats'
curl -s 'localhost:8080/cases/search?q=suppress+evidence'
curl -s localhost:8080/ledger/verify

curl -s -X POST localhost:8080/simulate -H 'content-type: application/json' -d '{
  "priors": {"evidence_strength":0.4,"charge_severity":0.6,"prior_record":0.2,
             "judge_propensity":0.5,"prosecutor_aggressiveness":0.8,"jury_propensity":0.5,
             "base_plea_months":24,"base_trial_months":72},
  "strategy": {"name":"suppress-then-trial","plea_discount":0.15,
               "suppression_bonus":0.10,"acquittal_bonus":0.10},
  "trials": 10000, "seed": 42}'

curl -s -X POST localhost:8080/simulate/from-case/22222222-2222-2222-2222-222222222222 \
  -H 'content-type: application/json' -d '{"trials":10000,"seed":42}'

# Ingest (fixture source — no CourtListener token required)
curl -s -X POST localhost:8080/ingest/run -H 'content-type: application/json' \
  -d '{"source":"fixture"}'
curl -s localhost:8080/ingest/status

# H3
curl -s localhost:8080/geo/kring/8828308281fffff?k=1

# Monell
curl -s 'localhost:8080/atlas/offices/fingerprint?office=Demo%20County%20DA&jurisdiction=CA'
curl -s 'localhost:8080/atlas/offices/monell-report?office=Demo%20County%20DA&jurisdiction=CA'

# Brady
curl -s -X POST 'localhost:8080/brady/derive/22222222-2222-2222-2222-222222222222'
curl -s -X POST 'localhost:8080/brady/reconcile/22222222-2222-2222-2222-222222222222'
curl -s 'localhost:8080/brady/lead-report/22222222-2222-2222-2222-222222222222'

# Trial penalty
curl -s 'localhost:8080/trial-penalty/offices?office=Demo%20County%20DA&jurisdiction=CA'
curl -s 'localhost:8080/trial-penalty/motion?office=Demo%20County%20DA&jurisdiction=CA'
curl -s 'localhost:8080/trial-penalty/disparity?group_a=Black&group_b=White&charge_category=drug'

# LASM evidence package (Markdown)
curl -s 'localhost:8080/lasm/package/22222222-2222-2222-2222-222222222222'

# Constitution / Bill of Rights (50-state dropdowns + advisory screen)
curl -s localhost:8080/constitution
curl -s localhost:8080/constitution/options
curl -s 'localhost:8080/constitution/jurisdictions?kind=state'
curl -s localhost:8080/constitution/provisions/amend.04
curl -s -X POST localhost:8080/constitution/resolve \
  -H 'content-type: application/json' \
  -d '{"clause_id":"amend.04.search_seizure","jurisdiction":"CA","court_level":"superior"}'
curl -s -X POST localhost:8080/constitution/screen/22222222-2222-2222-2222-222222222222
curl -s localhost:8080/constitution/screen/22222222-2222-2222-2222-222222222222

# Reckoning Engine (counsel reviews; public-record findings publish; engine does not charge)
curl -s localhost:8080/reckoning/wall
curl -s localhost:8080/reckoning/wall/aaaaaaaa-1111-4111-8111-111111111111
curl -s localhost:8080/reckoning/statutes
curl -s localhost:8080/reckoning/actors/aaaaaaaa-1111-4111-8111-111111111111/score
curl -s -X POST localhost:8080/reckoning/actors/aaaaaaaa-1111-4111-8111-111111111111/package \
  -H 'content-type: application/json' \
  -d '{"kind":"criminal_referral"}'
# Optional counsel hold (victim privacy / correction) — not a second publish opt-in:
curl -s -X POST localhost:8080/reckoning/actors/aaaaaaaa-1111-4111-8111-111111111111/publish \
  -H 'content-type: application/json' \
  -d '{"approved":false,"notes":"hold for victim-privacy review"}'
```

## Production

```bash
docker compose up --build
```

Runs Postgres, `vi-api` on `:8080`, and `vi-ingest` (fixture source on a 300s loop; set `INGEST_SOURCE=courtlistener` and `CL_API_TOKEN` for live dockets/opinions).

- `GET /health` — liveness
- `GET /ready` — database ping
- `GET /engines` — fourteen-engine catalog + row counts
- `BIND_ADDR` (default `0.0.0.0:8080`), `DATABASE_URL`, `DATABASE_MAX_CONNECTIONS`
- Graceful shutdown on SIGINT/SIGTERM
- Request tracing, 60s timeouts, permissive CORS (replace with an allow-list behind your gateway)

**Do not expose this service to the public internet without an auth gateway.** Defendant PII beyond the demo seed requires SOC 2-aligned controls, KMS, and row-level security (Phase 4).

## Guardrails (non-negotiable)

1. **Counsel review, then public accountability** — automated flags never publish. Licensed counsel must set `review_status = substantiated` on a public-record finding. That official-conduct record (name, office, bar/badge, citation, finding) then publishes on the Wall of Injustice. Counsel may hold a card for victim privacy or a correction. No photos, home addresses, or private contact data. The engine does not file charges. After conviction, sentencing memos advocate the statutory maximum — including life when the color-of-law titles authorize it (death resulting).
2. **Correlation ≠ causation** — every motion-facing statistic carries CI, *n*, and formula.
3. **Simulator honesty** — `p_conviction` weights are a transparent prior model. `POST /simulate/from-case/:id` fills them from office/judge public-record rates; calibrate further from `vi-correlation` before citing.
4. **Data licensing** — PACER fees/ToS; CourtListener/RECAP and state portals have their own terms. Race/ethnicity fields require counsel review.
5. **UPL** — LASM / Monell / trial-penalty / constitution-screen / Reckoning Markdown is attorney work product. The public portal stays informational. Constitutional screens and Reckoning packages are **not legal advice** and not charging documents. Sentencing advocacy is the sentence counsel will seek *if convicted*, bounded by the statute.
6. **Lawful inputs only** — court opinions, dockets, public settlements, bar records, FOIA disclosures. Sealed, juvenile, expunged, and non-public records are excluded.
7. **Incomplete holdings** — the Constitution text is complete (Preamble, Articles I–VII, Amendments 1–27). The interpretation snapshot is curated criminal-procedure doctrine plus 50-state charter analogs. Circuit splits stay `unsettled`. It is not a citator.

## Layout

```
crates/
├── vi-ledger/         Root Ledger
├── vi-correlation/    Pearson + odds ratios
├── vi-trustscript/    Abuse-detection DSL
├── vi-sim/            Seeded Monte Carlo
├── vi-geo/            H3 cells + k-ring
├── vi-db/             Pool, migrations, case context
├── vi-ingest/         Source trait + CourtListener + fixture
├── vi-tactics/        Tactic catalog + occurrence rates
├── vi-lasm/           Evidence-package renderer
├── vi-monell-atlas/   Pattern-and-practice atlas
├── vi-brady-recon/    Brady gap engine
├── vi-trial-penalty/  Trial Penalty Observatory
├── vi-constitution/   U.S. Constitution + Bill of Rights + 50-state analogs
├── vi-reckoning/      Individual accountability (named actors, referrals, public register)
└── vi-api/            Axum HTTP API
worker/                Cloudflare Worker landing page (Workers Builds)
wrangler.jsonc
```

This GitHub repo is still connected to the Cloudflare Worker named `vision`. The Worker is a static landing page (no case data). It exists so **Workers Builds** can compile; the engines themselves run via `vi-api`.
