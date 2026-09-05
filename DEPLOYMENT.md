# Deploying the public platform

The public site is a Cloudflare Worker in [`worker/`](worker/). It renders every
page at the edge and reads the Rust `vi-api` service over HTTPS. The Worker holds
no database and no secrets beyond an optional read token.

```
browser ──▶ Cloudflare Worker (worker/) ──▶ vi-api (Rust) ──▶ Postgres
             public pages + /api mirror       counsel-facing backend
```

## 1. Publish the Worker

The repository is connected to Cloudflare **Workers Builds** for the Worker named
`vision`, so a push to `main` builds and deploys automatically. Two alternatives:

```bash
# from a workstation with an authenticated wrangler
npm ci --legacy-peer-deps
npm run check          # typecheck + build
npm test               # 50 tests in the Workers runtime
npx wrangler deploy
```

Or through GitHub Actions — [`.github/workflows/deploy.yml`](.github/workflows/deploy.yml)
runs on `main` and on manual dispatch. It needs two repository secrets:

| Secret | Value |
|---|---|
| `CLOUDFLARE_API_TOKEN` | token created from the **Edit Cloudflare Workers** template |
| `CLOUDFLARE_ACCOUNT_ID` | the target account id |

The workflow also reads three optional repository *variables*: `VI_API_ORIGIN`
(passed to the deploy as a `--var`), `PUBLIC_SITE_URL` (used for the
post-deploy smoke test), and `environment` (a wrangler environment name, via
workflow dispatch input).

To publish a throwaway copy with no account at all — useful for reviewing a
branch — use a temporary preview account. Workflows and Durable Objects are not
available on those accounts, so deploy a config without them:

```bash
node -e "const f=require('fs');const c=JSON.parse(f.readFileSync('wrangler.jsonc','utf8').replace(/^\s*\/\/.*\$/gm,''));delete c.workflows;delete c.durable_objects;delete c.migrations;c.name='visioninjustice-preview';f.writeFileSync('wrangler.preview.json',JSON.stringify(c))"
npx wrangler deploy --temporary --config wrangler.preview.json
```

## 2. Point the Worker at the backend

Until `VI_API_ORIGIN` is set, every live panel renders its "not connected"
state and `/api/*` answers `503 backend_not_connected`. The doctrine, statute,
immunity, and API-documentation pages are complete without it, because that
content ships inside the Worker.

```bash
# permanent, in wrangler.jsonc
"vars": { "VI_API_ORIGIN": "https://api.example.org" }

# or per deploy
npx wrangler deploy --var VI_API_ORIGIN:https://api.example.org
```

Rules the Worker enforces on that value:

- HTTPS only, except `localhost` for `wrangler dev`.
- A trailing slash is stripped.
- A `3xx` from the backend is a failure, not a redirect to follow.
- Requests time out at 6 s for pages and 8 s for the `/api` mirror.

If the backend requires a bearer token for reads, store it as a secret — never
as a var:

```bash
npx wrangler secret put VI_API_TOKEN
```

## 3. Deploy the backend

The Worker needs `vi-api` reachable over HTTPS. Anywhere that runs a container
and can reach Postgres works; the repository ships a `Dockerfile` and
`docker-compose.yml`.

```bash
docker compose up -d db api
# DATABASE_URL, BIND_ADDR, DATABASE_MAX_CONNECTIONS are read from the environment
# migrations run automatically at startup via sqlx::migrate!
```

Put `vi-api` behind an authenticating gateway. The public Worker only ever
issues `GET`s against an allowlist of public paths, but the backend itself
exposes counsel operations — publication holds, package generation, rule runs,
ingestion — and those must never be reachable from the internet.

## 4. Custom domain

Add a route in `wrangler.jsonc` once DNS is on Cloudflare:

```jsonc
"routes": [
  { "pattern": "visioninjustice.org", "custom_domain": true },
  { "pattern": "www.visioninjustice.org", "custom_domain": true }
]
```

Set `SITE_NAME` and `CONTACT_EMAIL` vars at the same time — `CONTACT_EMAIL` is
published as the corrections and victim-privacy-hold channel, so it must reach
counsel.

## 5. Verify a deployment

```bash
curl -s https://<host>/healthz | jq          # edge + backend status
curl -sI https://<host>/ | grep -i content-security-policy
curl -s https://<host>/api/reckoning/wall | jq '.entries | length'
curl -s -o /dev/null -w '%{http_code}\n' https://<host>/api/flags   # must be 404
```

`/healthz` returns `200` when the edge is healthy and the backend is either
reachable or deliberately unconfigured, and `503` when a configured backend is
unreachable. It is the right target for an uptime check.

## What is deliberately absent

- **No writes at the edge.** Publication, holds, package generation, entity
  resolution, and ingestion exist only on the backend. The Worker answers `405`
  to any non-`GET`.
- **No unreviewed data.** `/flags`, `/reckoning/actors`, bare abuse scores,
  attorney work product, and case-level investigative leads are not in the
  `/api` allowlist. See [`worker/proxy.ts`](worker/proxy.ts) for the list and
  the reason attached to each exclusion.
- **No fabricated fallbacks.** When the backend is down, panels say so. An
  accountability register must not display a number it cannot source.

## Local development

```bash
npm ci --legacy-peer-deps
npm run dev:fixtures     # stub backend on :8788 with fictional records
npx wrangler dev --var VI_API_ORIGIN:http://localhost:8788
```

Regenerate the statute catalog after editing `crates/vi-reckoning/src/statutes.rs`:

```bash
npm run catalog          # writes worker/data/catalog.json
```

CI fails if that file drifts from the Rust source.
