# VisionInjustice

A Rust monorepo for **systemic criminal-justice accountability**. Twelve engines operate on **public records and substantiated findings only** — no OSINT, no leaked data, no auto-publication against named individuals.

The API is an MVP that is compile-time database-URL-free (runtime-checked SQL), hash-chained, and ready to sit behind a gateway for real-world testing. It is **not** a substitute for licensed counsel, and it ships without AuthN/Z (Phase 4).

## Engines

| Engine | Crate | Status |
|---|---|---|
| Root Ledger | `vi-ledger` | BLAKE3 hash-chained, append-only, full `verify()` |
| Case-law DB | schema + `vi-api` search | Generated `tsv` + GIN; opinion search live |
| Correlation | `vi-correlation` | Pearson + Fisher CIs, odds ratios, formula-audited |
| Tactics DB | schema | Ready; population is curation (Phase 2) |
| Abuse detection | `vi-trustscript` | Lexer/parser/evaluator + rule CRUD + ledger flags |
| Zero-day sim | `vi-sim` | Seeded Monte Carlo; weights are placeholder priors |
| H3 intelligence | `vi-geo` | Multi-res ladder at ingest; k-ring maps are Phase 3 |
| Telemetry / ingest | `vi-ingest` | `Source` trait + CourtListener client |
| JIT LASM | `vi-lasm` | Evidence-package Markdown with ledger provenance |
| Monell atlas | `vi-monell-atlas` | Pattern-and-practice fingerprint + §1983 scaffold |
| Brady recon | `vi-brady-recon` | Expected vs disclosed evidence; gaps are *leads* |
| Trial penalty | `vi-trial-penalty` | Distributions, disparity OR, draft motion template |

## Quick start

```bash
docker compose up -d db
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/visioninjustice

# pure tests — no DB required
cargo test -p vi-ledger -p vi-correlation -p vi-trustscript -p vi-sim -p vi-geo \
           -p vi-lasm -p vi-monell-atlas -p vi-brady-recon -p vi-trial-penalty

# with Postgres: applies migrations, then integration tests
cargo test -p vi-api

cargo run -p vi-api
```

Migrations run automatically on API (and ingest) startup via `sqlx::migrate!`.

### Demo against seeded data

```bash
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
```

## Production

```bash
docker compose up --build
```

- `GET /health` — liveness
- `GET /ready` — database ping
- `BIND_ADDR` (default `0.0.0.0:8080`), `DATABASE_URL`, `DATABASE_MAX_CONNECTIONS`
- Graceful shutdown on SIGINT/SIGTERM
- Request tracing, 60s timeouts, permissive CORS (replace with an allow-list behind your gateway)

**Do not expose this service to the public internet without an auth gateway.** Defendant PII beyond the demo seed requires SOC 2-aligned controls, KMS, and row-level security (Phase 4).

## Guardrails (non-negotiable)

1. **Defamation** — automated flags are never published against named prosecutors until an attorney-led Evidence Review Committee sets `review_status = substantiated`.
2. **Correlation ≠ causation** — every motion-facing statistic carries CI, *n*, and formula.
3. **Simulator honesty** — `p_conviction` weights are null-hypothesis priors. Calibrate per jurisdiction from `vi-correlation` before citing.
4. **Data licensing** — PACER fees/ToS; CourtListener/RECAP and state portals have their own terms. Race/ethnicity fields require counsel review.
5. **UPL** — LASM / Monell / trial-penalty Markdown is attorney work product. The public portal stays informational.
6. **Lawful inputs only** — court opinions, dockets, public settlements, bar records, FOIA disclosures. Sealed, juvenile, expunged, and non-public records are excluded.

## Layout

```
crates/
├── vi-ledger/         Root Ledger
├── vi-correlation/    Pearson + odds ratios
├── vi-trustscript/    Abuse-detection DSL
├── vi-sim/            Seeded Monte Carlo
├── vi-geo/            H3 cells
├── vi-db/             Pool, migrations, case context
├── vi-ingest/         Source trait + CourtListener
├── vi-lasm/           Evidence-package renderer
├── vi-monell-atlas/   Pattern-and-practice atlas
├── vi-brady-recon/    Brady gap engine
├── vi-trial-penalty/  Trial Penalty Observatory
└── vi-api/            Axum HTTP API
```
