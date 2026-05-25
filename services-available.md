# Services Already Available on This Machine — Agent Onboarding

Companion to `netlify-deployment.md`. This doc tells any agent (Claude Code, Cursor, etc.) **what's already set up on Austin's machine** so you don't waste a turn proposing new accounts, signing up for SaaS, or asking the user to "go create an API key for X." Many things are already wired — check here first.

> **Rule of thumb**: if a category below names a service, assume Austin already has an account, a billing relationship, and at least one project using it. The credentials live in either (a) the relevant project's `.env` / `.env.local`, (b) the project's Netlify env vars (`npx -y netlify-cli env:list` after `link`), or (c) Austin's password manager — **ask him**, don't sign up for a new account.

---

## How to Discover Credentials for a Service

Before asking the user for an API key, do this in order:

1. **Look at a sibling project that uses the same service.** Almost every service below is already wired into ≥1 project. Read its `.env.example`, `netlify.toml`, or `CLAUDE.md` — the env var name and shape are documented there.
2. **Check the running site's Netlify env vars.** From any linked project: `npx -y netlify-cli env:list`. Secrets are masked but names + presence are visible.
3. **Read the project's `CLAUDE.md` and `MEMORY.md`.** Most projects document which services they use and any gotchas.
4. **Then ask Austin.** Don't sign up for new accounts; he likely already has one.

---

## Adding a New Credential — The Localhost Entry Pattern

When a project genuinely needs a credential that isn't already wired up, **never paste the API key into the chat, the terminal, or any tool output.** Use the localhost-entry pattern so the secret never crosses the agent's view:

1. **Agent spins up a one-shot localhost form** in the project being worked on. Generic Express + HTML pattern (~30 lines of Node), bound to a free local port. Example invocation:
   ```bash
   node scripts/key-entry.js   # binds to e.g. http://localhost:3001
   ```
   The form has a single password-type input. On submit, the server appends `KEY_NAME=<value>` to `<project>/.env` (creating the file if needed), returns a 200, and shuts itself down. Per-project, no central tool — the script is small enough to scaffold on demand.
2. **Austin opens the URL, pastes the key, hits submit.** The key flows browser → localhost server → `.env` file. The agent only ever sees "submitted" / "written to .env" — never the value.
3. **Agent verifies presence (not value)** with something like `grep -c "^KEY_NAME=" <project>/.env` — confirm it returns `1`.
4. **Agent updates this file** per the rule in the next section before considering the task done.

**Why localhost over `read -s` in terminal:** a browser form keeps the secret out of shell history, terminal scrollback, and the conversation transcript. A `read -s` line is invisible at entry but the surrounding prompt + the env-var assignment can still leak into recordings, transcripts, or hook output.

---

## Documentation Rule — Update This File Every Time

**When the agent wires up a new credential — or when a credential rotates — `services-available.md` MUST be updated in the same change.** No exceptions. If the doc isn't updated, the credential is effectively undiscoverable next time and we re-burn the discovery cycle (or worse, accidentally sign up for a duplicate account).

Each new or updated credential entry must include all five of these fields:

| Field | Example |
|---|---|
| **Service name** | Resend |
| **Env var(s)** | `RESEND_API_KEY`, `RESEND_FROM_EMAIL` |
| **Project(s) using it** | `pate-ace`, `trucking-tickets` |
| **Local `.env` file location** | `~/Documents/Claude/pate-ace/.env` (and Netlify env vars on the deployed side) |
| **Service dashboard URL to retrieve / rotate** | `https://resend.com/api-keys` → "API Keys" → "Regenerate" |
| **Rate limits / gotchas / cost notes** | Free tier 100/day; Tier 2 $20/mo for 50K. Webhooks are Svix-signed — verify before processing. |

**The dashboard URL is the most important field** — it's what lets future-you (or future-agent) get back to the key without trial-and-error searching through dashboards. Always include the click-path, not just the root URL (e.g., `dashboard.stripe.com/apikeys` → "Reveal test key", not just `stripe.com`).

For credentials that are not API keys (database URLs, OAuth client IDs, etc.) the same five fields apply — substitute "where to retrieve / rotate" with whatever the equivalent is (Neon console → project → "Connection details", Google Cloud → APIs & Services → Credentials, etc.).

---

## MCP Servers (Claude Code Tools)

These are connected via Claude.ai sync (see the available tools list at session start). They're available to any agent in this environment without setup.

| MCP Server | Status | Use For |
|-----------|--------|---------|
| **Neon** | Ready | Postgres provisioning, branches, schema, run SQL, prepare migrations |
| **Netlify** | Ready (read/update) | Project, deploy, extension, team services. Note: `netlify-deploy-services-updater` is broken for builds — use the CLI per `netlify-deployment.md` |
| **Stripe** | Auth required | Payment management. Run authenticate flow before first use |
| **Gmail** | Ready | Search threads, drafts, labels (Austin's `austindmunday@gmail.com`) |
| **Google Calendar** | Ready | Events, scheduling |
| **Google Drive** | Ready | Search/read/copy files |
| **Gamma** | Ready | Generate presentations/docs (cannot edit existing — only generate new) |
| **Indeed** | Ready | Job + company + resume search |
| **Webflow** | Auth required | Site/content management |
| **Smartsheet** | Auth required | Sheets/projects |
| **Scholar Gateway** | Auth required | Academic research |
| **RobloxStudio** | Local | Custom MCP at `/Applications/RobloxStudio.app/Contents/MacOS/StudioMCP` (used by `RobloxMapCreator`) |

If a tool says "auth required" the first call returns a URL — pass it to Austin to complete OAuth.

---

## Database — Neon Postgres (default for new web projects)

**Neon is the default database for every new web project.** Supabase still backs one active mobile project (`austins-fleet-tracker`) — see the Supabase section below before assuming it's safe to delete that account.

- **Access patterns**:
  - Direct `DATABASE_URL` Postgres connection string (most projects)
  - HTTP via `@neondatabase/serverless` from Netlify Functions
  - Management API via Neon MCP (`mcp__Neon__*` tools — list projects, run SQL, prepare migrations, create branches)
- **Env var names**: `DATABASE_URL`, `NETLIFY_DATABASE_URL`, `NEON_API_KEY`
- **Existing projects**: `pate-ace` (project `pate-ace-catalog`, ID `wild-hall-43125776`), `austins-haul-rate-calc`, `loadhog`, `austin-munday-realty`, `Reverse-Search`, `permit-data` / PermitSphere (project `permitsphere`, ID `young-rain-68992800`), `BORROWPIT-SAR`, `Tokenmaxxen-app` (api), `aidvocate`
- **Gotchas**:
  - Use connection pooling (`?sslmode=require&channel_binding=require`) for serverless functions — direct connections leak under cold-start fan-out
  - Branches are free — use them for migration testing via `mcp__Neon__create_branch` then `prepare_database_migration` before touching prod

### Supabase (mobile-only, do NOT use for new web projects)

- **Status**: **Actively in use** by `austins-fleet-tracker` (iOS) — project `blhmqfhcksamtdipltxd` (West Oregon, Free tier, Postgres 17.6). 8 tables, 17 RLS policies, free-tier truck-limit trigger, auto-create-`user_profile` trigger, Realtime publication on `truck_pings` / `shifts` / `assignments`. This is **not** "abandoned" — it is the live backend for the fleet tracker iOS app. Do not delete the project.
- **Env vars** (in `~/Documents/Claude/austins-fleet-tracker/.env`, also flowed into iOS via `scripts/sync-secrets.sh` → `Secrets.xcconfig` → `Info.plist` `$(SUPABASE_URL)` / `$(SUPABASE_ANON_KEY)`): `SUPABASE_PAT`, `SUPABASE_PROJECT_REF`, `SUPABASE_URL`, `SUPABASE_ANON_KEY`, `SUPABASE_SERVICE_ROLE_KEY`, `SUPABASE_DB_PASSWORD`, plus `SUPABASE_LEGACY_ANON_KEY` / `SUPABASE_LEGACY_SERVICE_ROLE_KEY` from a prior project (kept for rollback).
- **Why Supabase here and not Neon**: chosen for built-in Sign in with Apple auth provider, Realtime websocket on `truck_pings`, and Management API project provisioning from the localhost-keys-form flow. Migrating to Neon would require rebuilding all three.
- **Gotchas**:
  - Supabase Management API + Python `urllib` → WAF block. Always use `curl` with a non-default User-Agent for programmatic SQL/migration runs.
  - Personal Access Token (`SUPABASE_PAT`) lives at `https://supabase.com/dashboard/account/tokens` — needed for Management API project/migration ops, not for runtime client SDK.
- **For new projects**: default to Neon. Only reach for Supabase if the project needs Supabase Auth, Edge Functions, or Realtime websockets and rebuilding those on Neon would burn more than a day.

---

## Cloud Storage

### Cloudflare R2 (S3-compatible object storage)

- **Account ID** (shared across projects): `3d5ab6d984434c89a3f77fa3623c409d`
- **Existing buckets**:
  - `austins-radar` — weather tile cache, served via Cloudflare Worker `austins-radar-tiles-cache` at `tiles.austinsradar.com`
  - `listing-photos` — pate-ace catalog images at `https://photos.austinsidxcombinator.com`, paths `pate-ace-catalog/hardware/<sku>.jpg` and `pate-ace-catalog/furniture/<sku>.jpg`
  - `tokenmaxxen-images` — Tokenmaxxen-app post images, served at `https://img.tokenmaxxen.com` (`R2_PUBLIC_BASE_URL`)
  - `permitsphere` — private (no public domain binding), holds permits / code_cases / contacts / skip-trace cache as JSON keyed by canonical IDs. Browsers never read R2 directly; Netlify Functions proxy via S3 API. Token: bucket-scoped to `permitsphere`, Object Read & Write. Mint at `dash.cloudflare.com/3d5ab6d984434c89a3f77fa3623c409d/r2/api-tokens`
  - `borrowpit-r2` — BORROWPIT-SAR cadence-volumetrics outputs (NISAR L-band InSAR over construction-aggregate stockpiles)
- **Env var names**: `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_R2_ACCESS_KEY_ID`, `CLOUDFLARE_R2_SECRET_ACCESS_KEY`, `CLOUDFLARE_R2_BUCKET_NAME`, `R2_CDN_BASE` / `R2_PUBLIC_BASE_URL`
- **Known gotcha**: R2 PUT from inside Netlify Functions has ~14% intermittent `SignatureDoesNotMatch` rate. Workaround used in pate-ace: upload via local Node CLI script, not from the function. Reads/proxies from R2 work fine.
- **CDN headers**: when uploading product images, set `x-amz-meta-*` for self-identification (sku, project, content_hash) — pate-ace uses this for cache invalidation.

### Netlify Blobs (KV store, auto-injected on Netlify)

- No setup needed inside a Netlify Function — `@netlify/blobs` reads creds from auto-injected env. From outside Netlify (CLI scripts) you need `NETLIFY_API_TOKEN` + site ID.
- **Existing usage**: pate-ace product cache, austin-munday-realty MLS county blobs (one blob per county for parallelism), image proxy cache (7-day TTL), geocoding cache, sync state, trucking-tickets artwork (keyed by Stripe session ID).
- Use Blobs for ephemeral caches and small state. Use Neon for relational data. Use R2 for large files (>1MB) and anything CDN-served.

---

## Email Sending

### Resend (default for transactional + campaigns)

- **Env vars**: `RESEND_API_KEY`, `RESEND_FROM_EMAIL`, `RESEND_AUDIENCE_ID` (broadcasts only)
- **Existing projects**: `pate-ace` (newsletter campaigns + order confirmations + admin composer at `/admin/campaigns.html`), `trucking-tickets` (order emails, sample requests)
- **Webhooks**: `pate-ace` has `/.netlify/functions/resend-webhook` — Svix-signed, idempotent. Copy that pattern for new projects.
- **Audiences**: broadcasts can target either Resend Audience or a Neon `newsletter_subscribers` table — pate-ace mirrors both.

### SendGrid

- **Env var**: `SENDGRID_API_KEY`
- **Project**: `loadhog` (transactional reports, notifications). Configured but placeholder values in stubs — confirm with Austin if a new project should use SendGrid vs. Resend (default Resend unless there's a reason).

### Netlify Forms

- For static contact / sample-request forms, use Netlify Forms instead of writing a function. Notifications configured per-site in the Netlify UI. Used by Xtreme-Concrete-Recycling, austin-munday-realty contact pages, trucking-tickets sample requests.

---

## Payments

### Stripe

- **Env vars**: `STRIPE_SECRET_KEY`, `STRIPE_PUBLISHABLE_KEY` (or `VITE_STRIPE_PUBLIC_KEY` in Vite projects), `STRIPE_WEBHOOK_SECRET`
- **Existing projects**: `loadhog` (multi-tenant SaaS subscriptions + usage billing), `trucking-tickets` (Stripe Checkout for NCR ticket orders)
- **Webhook pattern**: `/.netlify/functions/stripe-webhook` — verify signature using `STRIPE_WEBHOOK_SECRET`, return 200 fast, then enqueue heavy work to a `-background` function.
- **MCP**: Stripe MCP available (auth required) for account introspection.

### PayPal

- **Env vars**: `PAYPAL_ENV` (`sandbox` | `live`), `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET`, `NEXT_PUBLIC_PAYPAL_CLIENT_ID` (browser SDK), `PAYPAL_WEBHOOK_ID`
- **Existing projects**:
  - `pate-ace` (sandbox configured 2026-05-03, awaiting business credentials swap before live)
  - `aidvocate` (in `~/Documents/Claude/grant-appeal/aidvocate/`) — primary checkout, $59 self-serve letter. **Live credentials in place as of 2026-05-09** (`PAYPAL_ENV=live`). For local sandbox testing, swap `PAYPAL_ENV=sandbox` and use the sandbox client ID/secret pair from PayPal Developer Dashboard
- **Preference vs Stripe**: Austin's preferred consumer-facing checkout for low-volume / one-time purchases is PayPal (lower friction for non-tech buyers, no card form). Stripe stays the default for SaaS subscriptions and B2B invoicing. When in doubt for a new project, ask which checkout — don't auto-pick Stripe.
- **Webhook pattern**: verify `PAYPAL_WEBHOOK_ID` via the PayPal verify-webhook-signature endpoint (no shared secret like Stripe's).

### RevenueCat (mobile subscriptions on top of StoreKit 2)

- **Env vars**: `REVENUECAT_PUBLIC_API_KEY`, `REVENUECAT_SECRET_KEY`
- **Projects**: `austins-fleet-tracker` (planned, env stubs in place), `Austins-Radar` (deferred). Apple Small Business Program enrolled — 15% commission tier.

---

## SMS / Telephony

### Telnyx

- **Env vars**: `TELNYX_API_KEY`, `TELNYX_PHONE_NUMBER`
- **Project**: `loadhog` (driver SMS notifications, dispatch)
- **Twilio is NOT set up.** If a project asks for SMS, default to Telnyx unless Austin says otherwise.

---

## AI / ML APIs

| Service | Env var | Used by | Notes |
|---------|---------|---------|-------|
| **OpenAI** | `OPENAI_API_KEY` | `loadhog` (HogBot chat), `Educational-YT-Videos` (`gpt-image-2` for scene generation via `pipeline/scripts/gen-image-openai.sh`) | LoadHog returns 503 if key missing — no fallback. Educational-YT-Videos requires it in `commission.sh` |
| **Anthropic / Claude API** | `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL_LETTER`, `ANTHROPIC_MODEL_OCR` | `aidvocate` (in `~/Documents/Claude/grant-appeal/`) — Sonnet 4.6 for letter generation, Haiku 4.5 vision for OCR/extraction. Add to other projects as needed | Default to latest models per machine guidance: Opus 4.7, Sonnet 4.6, Haiku 4.5. Use prompt caching on repeated system+context blocks (template text, schema, statutes) — typically ~80% cost reduction |
| **Replicate** | `REPLICATE_API_TOKEN`, `REPLICATE_USERNAME` | `Educational-YT-Videos` | Custom Flux LoRA `austinmunday/ancient-humans-style:726f...`. `LORA_TRIGGER_WORD`, `FLUX_LORA_VERSION`. Default budget $8/run. nano-banana (Google) is the configured fallback |
| **ElevenLabs** | `ELEVENLABS_API_KEY`, `ELEVENLABS_VOICE_ID`, `ELEVENLABS_MODEL` | `Educational-YT-Videos` | Default voice `mhgBlD8CmCSdwLDOIJpA` (Pulse). Models: `eleven_v3` (quality) or `eleven_multilingual_v2` (fallback) |
| **OpenAI Codex API (GPT-5.5-Pro — current frontier)** | `OPENAI_CODEX_API_KEY` — sourced from `~/.config/trace-distiller/openai-codex.env` (mode 0600, **outside** any project dir) | `api-agent-infra` (Trace Distiller Phase 0 two-labeler agreement gate — wired with gpt-5.3-codex historically; new work should default to gpt-5.5-pro). Called via OpenAI **Responses API** (not Chat Completions) | **This is Austin's "second-opinion" model.** See "Cross-Check / Second-Opinion Pattern" section below. Default model: `gpt-5.5-pro` (pinned dated alias: `gpt-5.5-pro-2026-04-23`) |

### Cross-Check / Second-Opinion Pattern — calling GPT-5.5-Pro to double-check Claude

Austin sometimes wants Claude-based agents to **independently re-verify their own work against a different model family** before committing to a non-trivial decision. OpenAI's frontier reasoning model — **`gpt-5.5-pro`** as of 2026-05-23 — is the configured second opinion, called via the Codex API key. The pattern is borrowed from `api-agent-infra/docs/BUILD-ROADMAP.md` §6.3 step 4 — a two-labeler agreement gate where Claude (surfacer) and the OpenAI checker re-label the same inputs against the same protocol, then a disagreement diagnostic is produced.

**Always use the most advanced model the key can access.** Verify at the start of any new cross-check workflow by listing models:
```bash
source ~/.config/trace-distiller/openai-codex.env
curl -sS https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_CODEX_API_KEY" \
  | python3 -c "import json,sys; print('\n'.join(sorted(m['id'] for m in json.load(sys.stdin)['data'] if 'gpt-5' in m['id'])))"
```
Pick the highest-numbered `*-pro` variant (currently `gpt-5.5-pro`). When a new frontier ships, **update this doc** with the new ID + verification date — don't quietly keep using a stale string.

**Model selection rule of thumb** (highest to lowest cost/capability):
- `gpt-5.5-pro` — frontier reasoning, default for cross-check and hard adjudication
- `gpt-5.5` — frontier non-pro, ~5× cheaper, use for high-volume label batches where pro is overkill
- `gpt-5.4-mini` — fast/cheap subagent calls, low-stakes sanity checks
- `gpt-5.3-codex` — legacy Codex-specific build; only use to reproduce api-agent-infra Phase 0 results exactly

**When Austin signals he wants this:** phrases like "double-check yourself," "get a second opinion," "have codex verify," "cross-check with the other model," "two-labeler gate," or any direct reference to the api-agent-infra Phase 0 methodology. Also volunteer it proactively for: labeling / classification work where Claude's confidence is borderline, ambiguous protocol interpretation, anything where being wrong is expensive (architecture decisions, irreversible data labels, gold-set entries, contract drafts). **Do not** burn it on cheap-to-undo work — `gpt-5.5-pro` is the most expensive model in the catalog and only earns its keep when the decision is hard or costly to reverse.

**How to call it (Python, the proven path):**

1. **Source the key** at runtime — never read it via `os.environ` directly without sourcing, since it isn't in any project's `.env.local` yet:
   ```bash
   source ~/.config/trace-distiller/openai-codex.env
   ```
   That file exports `OPENAI_CODEX_API_KEY=sk-...`. Mode 0600. If running from a long-lived agent process, read the file directly with `pathlib.Path("~/.config/trace-distiller/openai-codex.env").expanduser().read_text()` and parse the `KEY=value` line — same result, no shell needed.

2. **Use the OpenAI Responses API** (not Chat Completions — Chat Completions support is being deprecated; Responses is the supported surface for reasoning models):
   ```python
   from openai import OpenAI
   client = OpenAI(api_key=os.environ["OPENAI_CODEX_API_KEY"])
   resp = client.responses.create(
       model="gpt-5.5-pro",  # frontier; pin to gpt-5.5-pro-2026-04-23 if you need reproducibility
       input=[
           {"role": "system", "content": SYSTEM_PROMPT},   # same protocol Claude received
           {"role": "user",   "content": USER_PROMPT},      # same input record Claude received
       ],
   )
   checker_output = resp.output_text
   ```

3. **Independence is the whole point.** Give the checker the same inputs Claude saw + the same protocol/rubric, but **never** show the checker Claude's answer. The value of a second opinion collapses to zero if the checker is anchored on the first model's output.

4. **Agreement gate**: parse both outputs into the same schema, compare on the decision field(s). On agreement → proceed. On disagreement → either halt for Austin's adjudication or log to a `disagreement.jsonl` and surface the conflict in the summary.

5. **Rate limits / cost**: `gpt-5.5-pro` is a reasoning model — calls are slower (seconds to tens of seconds, not ms) and substantially pricier than Claude Sonnet per call. In the api-agent-infra Phase 0 run (with gpt-5.3-codex, similar profile), 21/892 calls hit transient rate-limit errors. For batches >50 calls: build in exponential-backoff retry, checkpoint progress to disk every N calls so a rate-limit blowup doesn't cost the whole run, and **estimate spend upfront** (a 1k-row gold-set pass at pro pricing can land in the hundreds of dollars — confirm budget with Austin before kicking off).

**Reference implementation lives at**: `~/CascadeProjects/experiments/v02-gate-test/labelers.py` (checker class + Responses API call + retry logic) and `~/CascadeProjects/experiments/v02-gate-test/agreement_gate.py` (binary + adjudication-value + field-level consensus). Swap the model string from `gpt-5.3-codex` to `gpt-5.5-pro` when reusing this scaffold for new cross-check work — everything else (key sourcing, Responses API call shape, retry logic, schema) carries over unchanged.

**Do not confuse with the regular OpenAI key.** `OPENAI_API_KEY` (loadhog, Educational-YT-Videos) is a **separate billing account** wired for `gpt-image-2` and chat use cases. `OPENAI_CODEX_API_KEY` is the Codex-specific key — different dashboard, different rate-limit bucket, full access to the gpt-5.x family including `*-pro` variants. Don't reuse one for the other.

---

## External Data APIs

### NASA Earthdata Login (EOSDIS bearer token)

- **Env var**: `EARTHDATA_TOKEN` (Bearer)
- **Project**: `BORROWPIT-SAR` (NISAR L-band InSAR ingest for construction-aggregate stockpile volumetrics)
- **Local `.env` location**: `~/Documents/Claude/BORROWPIT-SAR/.env` (gitignored; `.env.example` documents the var). Validated locally by `borrowpit doctor`.
- **Dashboard / rotate**: `https://urs.earthdata.nasa.gov` → sign in → `users/<username>/user_tokens` → "Generate Token" → copy the JWT. Free account.
- **Gotchas**:
  - **~60-day token lifetime.** Rotate on expiry — there's no auto-refresh on EDL.
  - Python ecosystem: `asf-search>=7,<9` and `earthaccess>=0.10,<0.15` are the Python clients listed in `BORROWPIT-SAR/pyproject.toml`. Both accept the bearer token via env or login keyring.
  - For NISAR / Sentinel-1 SAR pulls specifically, use ASF Search (Alaska Satellite Facility archive); for the broader EOSDIS catalog use `earthaccess`. Same token works for both.

### Socrata Open Data (`data.{state}.gov`, `data.{city}.gov` portals)

- **Env var**: `SOCRATA_APP_TOKEN`
- **Project**: `permit-data` / PermitSphere (NJ statewide ETL via `app/scripts/etl-nj.mjs`, Orlando code enforcement via `app/scripts/etl-orlando-code.mjs` — Socrata dataset `k6e8-nw6w`)
- **Local `.env` location**: `~/Documents/Claude/permit-data/app/.env.local` (intake via localhost-form pattern; `.env.example` documents the var)
- **Dashboard / rotate**: register at the specific portal (e.g. `https://data.nj.gov` → sign in → profile → developer settings → "Generate App Token"). Free. Per-portal token — NJ's token does not work on Orlando's portal; you need one per `data.<jurisdiction>.gov` host you pull from.
- **Gotchas**:
  - **Without a token, Socrata throttles bulk pulls by IP.** The 2.7M-row NJ Construction Permit backfill will not complete unthrottled — token is required, not optional.
  - Whether a token is required vs. just rate-limit-relieving depends on the portal — assume required for any backfill >10k rows.

### BatchData (property skip-trace / owner contact enrichment)

- **Env var**: `BATCHDATA_API_KEY`
- **Project**: `permit-data` (PermitSphere v1.6 paid add-on — owner-contact enrichment on code-enforcement cases where Orlando's dataset has no PII). Wired into `app/netlify/functions/owner-skip-trace.mjs` + `skip-trace-status.mjs` + `_lib/batchdata`.
- **Local `.env` location**: `~/Documents/Claude/permit-data/app/.env.local`. **No live value yet** — `BATCHDATA_API_KEY=` empty stub in `.env.example`. When unset, `/api/skip-trace-status` returns `{ ready: false }` and the DetailPanel disables the "Find owner contact" button with a tooltip (graceful degradation, intentional).
- **Dashboard / rotate**: `https://app.batchdata.com` → sign in → API keys / wallet settings.
- **Cost / rate notes**:
  - **$50 wallet minimum**, pay-as-you-go per lookup. Wallet must be funded before any successful call.
  - Results cached in R2 at `skip-traces/{sha256(parcelId:ownerNameNorm)}.json` with **90-day TTL** enforced at read by `fetchedAt`. Always check the cache before billing a fresh lookup.
  - Production wiring deferred until PermitSphere paywall is live (don't fund the wallet pre-launch).

### GitHub OAuth (PKCE for Tokenmaxxen-app)

- **Env vars**: `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`
- **Project**: `Tokenmaxxen-app/api` (knowledge-vault backend — alternate auth path alongside Sign in with Apple for the macOS app, audience `com.austinmunday.tokenmaxxen`)
- **Local `.env` location**: `~/Documents/Claude/Tokenmaxxen-app/api/.env.local` (stubs in `.env.example`; not fully provisioned yet as of 2026-05-23)
- **Dashboard / rotate**: `https://github.com/settings/developers` → OAuth Apps → select the Tokenmaxxen-app app → "Generate a new client secret". PKCE-enabled (no implicit-flow fallback).
- **Gotchas**: PKCE OAuth Apps don't ship a long-lived client secret to the client — the secret stays server-side, client uses code-verifier/code-challenge. Don't bake the secret into the macOS app bundle.

---

## Maps & Geolocation

### Google Maps Platform

- **Env vars**: `GOOGLE_MAPS_API_KEY`, `GOOGLE_MAPS_API_KEY_TWO` (backup with Geocoding + Places enabled), `GOOGLE_GEOCODING_KEY`
- **Project**: `austin-munday-realty` (geocoding, Places ZIP search, embedded map), `Xtreme-Concrete-Recycling` (embed)
- **Gotcha**: keys are scoped per-API. The realty site hit a "geocoding works on key A, Places fails because Places only enabled on key B" issue — env precedence locked as `GOOGLE_GEOCODING_KEY` → `GOOGLE_MAPS_API_KEY_TWO` → `GOOGLE_MAPS_API_KEY`. Don't restrict keys further without coordinating.

### Mapbox

- **Env vars**: `MAPBOX_ACCESS_TOKEN`, `MAPBOX_PUBLIC_TOKEN`, `MAPBOX_SECRET_TOKEN`
- **Projects**: `Reverse-Search` (rendering + geocoding, falls back to Nominatim if unset), `austins-fleet-tracker` (planned: Mapbox Directions truck profile with `max_height`/`max_width`/`max_weight`)

### Leaflet + CARTO / OpenStreetMap tiles (no-key option)

- **Stack**: `leaflet` + `react-leaflet` (frontend only). Tiles served from public CDNs — **no API key, no account, no env var**.
- **Tile URL pattern (CARTO Voyager, light SaaS look)**: `https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png` with attribution `© OpenStreetMap contributors · © CARTO`.
- **Project**: `fl-tax-deed-tracker` (`~/Documents/Claude/fl-tax-deed-tracker/`) — map-first SaaS dashboard, ~1,200 FL parcels rendered as `CircleMarker`s with status-bucket popups. See `app/src/Map.tsx` for the canonical pattern (TileLayer + jittered centroid fallback + `useMap` pan/resize hooks + Google Maps deep-link from popup).
- **When to use over Google Maps / Mapbox**: pure plotting / clustering use cases where you control the marker layer, don't need Directions/Places/geocoding from the map provider, and don't want a billing-relationship token in the bundle. Geocoding for these projects is sourced separately (per-county Property Appraiser ArcGIS REST, FGIO statewide cadastral, county centroid fallback) — not from the tile vendor.
- **Limits to know**: CARTO and OSM public tile endpoints are best-effort and ask that high-traffic users self-host or pay. Fine for MVP / low-thousands DAU; if a project takes off, swap in Mapbox tiles (same Leaflet client, just change the `TileLayer` URL + add token) before the polite-use line gets crossed.
- **Marker scaling note from fl-tax-deed-tracker**: at ~1k parcels statewide, dense counties stack visibly. Plan to add `react-leaflet-cluster` once any single county pushes past ~150 markers in view.
- **Don't forget the CSS import.** Leaflet's stylesheet must be loaded globally or the map renders broken (zoom controls misaligned, tiles offset). In fl-tax-deed-tracker it's `@import "leaflet/dist/leaflet.css";` at the top of `app/src/index.css`. Equivalent: `import "leaflet/dist/leaflet.css"` in your entry file.

---

## Real Estate / MLS Data — MLS Grid v2

- **Env var**: `MLS_GRID_API_TOKEN` (Bearer)
- **Base URL**: `https://api.mlsgrid.com/v2`
- **Project**: `austin-munday-realty` (Stellar MLS IDX)
- **Hard rate limits — non-negotiable, account-suspendable** (suspended 3× already):
  - **2 RPS max** (≥500ms between calls — pace it)
  - 7,200 requests/hour
  - 40,000 requests/24h
- **Compliance gotchas** (read `STELLAR-MLS-COMPLIANCE.md` in that project before touching MLS code):
  - `ListOfficeName` MUST display on every listing card/detail
  - Filter must include `MlgCanView eq true and contains(MlgCanUse,'IDX')`
  - Public IDX pages: `StandardStatus eq 'Active'` only
  - Use `ListingId` for filtering, not `ListingKey`
  - Max 5,000 records/req (1,000 if `$expand=Media`)
  - Delta sync uses `ModificationTimestamp`
  - Photo URLs must be proxied through `/.netlify/functions/listing-image` (never hotlink MediaURL — credentials embedded)
  - **Never call MLS Grid from a user-facing function.** All MLS data is cached in Neon + Netlify Blobs and refreshed by background crons (sync at `:00`, coordinate backfill at `:08`/`:38` at 0.02 RPS)

---

## Mobile / Native

### Apple AdMob

- **App ID** (Austins-Radar iOS): `ca-app-pub-9271677932509933~8923305533`
- **Banner unit**: `ca-app-pub-9271677932509933/2374209056`
- **Test banner** (Debug builds): `ca-app-pub-3940256099942544/2934735716`
- **Initialization order is locked**: UMP → ATT → AdMob init. Don't reorder. GDPR consent message is deferred until App Store live; EU users get non-personalized ads in the meantime.

### Apple Small Business Program

- Enrolled (15% commission). Applies to `Austins-Radar` and `austins-fleet-tracker`.

### Sign in with Apple

- Used by `austins-fleet-tracker`. **No Google Sign-In** in mobile apps (App Store guideline 4.8 alignment).

---

## Hosting Targets

| Platform | Used by | Deploy command / notes |
|---------|---------|------------------------|
| **Netlify** | most static + Functions sites (austin-munday-realty, pate-ace, trucking-tickets, Xtreme-Concrete-Recycling, Austins-Radar marketing site, Reverse-Search, austins-haul-rate-calc, etc.) | See `netlify-deployment.md` |
| **Railway** | `loadhog` only | Auto-deploy on push to `main`. `railway.json` controls build. Build: `npm run build` (Vite + esbuild), start: `node dist/index.js` |
| **Fly.io** | `Austins-Radar/austins-radar-tiles` (MRMS tile pipeline) | App `austins-radar-tiles`, IAD region, 2× shared-cpu-2x/2GB. `fly deploy --remote-only --app austins-radar-tiles`. SSO account — `fly auth login` (PATs don't work) |
| **Cloudflare Workers** | Austins-Radar tile cache | Worker `austins-radar-tiles-cache`, route `tiles.austinsradar.com/*`, R2 binding `BUCKET → austins-radar`. Deploy: `wrangler deploy` from `austins-radar-tiles/worker/` |
| **Cloudflare Pages** | (legacy) `austins-radar-site` duplicate | Marked for cleanup — primary is Netlify |
| **App Store / Play Store** | Austins-Radar, austins-fleet-tracker | Manual. Apple Small Business tier |

**Default for new web projects: Netlify.** Only divert to Railway if the project needs a long-running container (loadhog does), or to Fly if it needs persistent compute close to data sources.

---

## Domains & DNS — Cloudflare Registrar

Domains registered through Cloudflare:

- `austinsradar.com` (zone ID `a07c0c58f20255ed5532c640a51392d9`) — DNS-only (no proxy), Smart Tiered Caching enabled
- `haultickets.com` (registered 2026-05-01, DNS cutover pending)
- `austinmundayrealestate.com` (live, points to Netlify site `kaleidoscopic-dragon-5efa57`)
- `loadhog.pro` (Railway)
- `austinsidxcombinator.com` (R2 photo CDN endpoint)

**No Namecheap, no GoDaddy.** New domains go through Cloudflare.

---

## What's NOT Set Up (Don't Assume)

To save time guessing in the other direction:

- **Twilio** — not configured. SMS goes through Telnyx.
- **Supabase for new web projects** — Neon is the default. Supabase is **live** for `austins-fleet-tracker` only (iOS, project `blhmqfhcksamtdipltxd`); see the Supabase subsection under Database for why it stays.
- **PostHog / Plausible / GA / Mixpanel** — explicitly deferred. No analytics in v1 of any project.
- **Vercel** — not used. Netlify is the static-host default.
- **AWS account** — not directly wired. R2 is the S3-equivalent.
- **Sentry / Datadog** — not configured. Logging is via `console.*` in Functions + `netlify logs:function`.
- **Auth0 / Clerk** — not used. Sign in with Apple (mobile), NextAuth + session cookies (Reverse-Search). No central auth provider.
- **TelemetryDeck** — planned for Austins-Radar but not yet wired.
- **Webflow / Shopify / WordPress** — not used. Pattern is "Netlify static + vanilla JS + Functions."

If a task seems to need one of these, **ask before signing up** — there's usually an existing alternative above.

---

## Quick Project → Services Map

| Project | DB | Pay | Email | Maps | Storage | Host |
|---------|----|----|-------|------|---------|------|
| austin-munday-realty | Neon | — | — | Google Maps | Netlify Blobs | Netlify |
| austins-haul-rate-calc | Neon | — | — | — | — | Netlify |
| loadhog | Neon | Stripe | SendGrid | — | — | Railway |
| pate-ace | Neon | PayPal | Resend | — | R2 + Blobs | Netlify |
| trucking-tickets | — | Stripe | Resend | — | Blobs | Netlify |
| Reverse-Search | Neon | — | — | Mapbox | — | Netlify |
| austins-fleet-tracker | **Supabase (live)** | RevenueCat (planned) | — | Mapbox (planned) | — | iOS app |
| Austins-Radar | — | StoreKit 2 | — | — | R2 + CF Worker | iOS + Netlify + Fly + CF |
| Xtreme-Concrete-Recycling | — | — | — | Google embed | — | Netlify |
| Educational-YT-Videos | — | — | — | — | — | local (Replicate + ElevenLabs + OpenAI gpt-image-2) |
| aidvocate (grant-appeal) | Neon | PayPal | Resend | — | R2 (uploads + letters buckets) | Netlify |
| fl-tax-deed-tracker | — (static JSON) | — | — | Leaflet + CARTO/OSM | — | Netlify |
| permit-data (PermitSphere) | Neon (`young-rain-68992800`) | Stripe (subscription) | Resend | Mapbox + Google Maps (Street View on demand) | R2 (`permitsphere`) | Netlify |
| BORROWPIT-SAR | Neon | — | — | — | R2 (`borrowpit-r2`) + NASA Earthdata | Netlify (planned) |
| Tokenmaxxen-app (api) | Neon | — | Resend (mention fallback) | — | R2 (`tokenmaxxen-images` @ img.tokenmaxxen.com) | Netlify |
| api-agent-infra (Trace Distiller) | — | — | — | — | — | strategy workspace (calls Anthropic + **OpenAI Codex** for two-labeler gate) |

---

## Bottom Line for Agents

When the user asks for a feature that needs an external service:

1. **Find the matching row above.** Almost everything has a precedent.
2. **Copy the env-var convention from a sibling project** rather than inventing new names.
3. **Don't propose new SaaS** — Resend, Neon, R2, Stripe, Telnyx, Cloudflare, Netlify cover ~95% of what these projects need.
4. **Respect the rate limits and gotchas above** — especially MLS Grid (2 RPS) and R2 PUT from Functions (use local CLI).
5. **For a NEW credential: use the localhost-entry pattern** described above — never accept secrets in chat or terminal output. For an EXISTING credential: follow the discovery order (sibling project → Netlify env list → CLAUDE.md), and only ask Austin for the value as a last resort.
6. **After wiring a new credential, update this file in the same change** — env var name, project(s), `.env` location, dashboard URL with click-path, and any rate limits / gotchas. Non-negotiable. If the doc isn't updated, the work isn't done.
