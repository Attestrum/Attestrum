# Netlify Deployment — Agent Onboarding

Context for any agent (Claude Code, Cursor, etc.) deploying a new project on Austin's machine. The Netlify account, CLI, and team are already set up — this doc tells you how to plug a fresh project into them.

> **Also read `services-available.md` (sibling file).** It documents every other service already wired on this machine — Neon Postgres, Cloudflare R2, Resend, Stripe, Telnyx, Mapbox, Google Maps, MLS Grid, Replicate, ElevenLabs, Cloudflare Workers, Fly.io, Railway, plus all available MCP servers (Neon, Stripe, Gmail, Google Calendar, Drive, Gamma, Indeed, Webflow, Smartsheet, Scholar Gateway, RobloxStudio). Don't propose signing up for new SaaS without checking that file first.

---

## What's Already In Place

- **Netlify account**: austindmunday@gmail.com, logged in via the CLI on this machine.
- **Team**: `Rank and Rent` (Team ID `69a2633551acc4f9964589eb`, **account slug `austindmunday`** — note the slug does NOT match the team name; CLI calls like `sites:create --account-slug` need the slug, not the team name). All new sites should land here unless explicitly told otherwise.
- **CLI**: works via `npx -y netlify-cli` — no global install, no auth flow needed. If a command ever prompts for login, something is wrong (don't re-auth, ask Austin).
- **Existing site for reference**: `kaleidoscopic-dragon-5efa57` (Site ID `cb94e1be-06be-473a-b534-5363969d4d46`, live at austinmundayrealestate.com) — the rank-and-rent real estate template. New agent sites are clones of this pattern.

You do **not** need to create accounts, generate tokens, or set up auth. Just link the new project and deploy.

---

## The One Command That Works

```bash
npx -y netlify-cli deploy --prod --dir=. --functions=netlify/functions
```

Run from the project root. `--dir` points at the publish folder (`.` for static, `dist` / `build` / `.next` for built sites). Drop `--functions` if the project has none.

**Do NOT use the `netlify-deploy-services-updater` MCP tool.** Server-side build fails on function bundling. The CLI is the only reliable path on this machine — confirmed across multiple projects.

---

## Wiring Up a Fresh Project

1. **`cd` into the project root.**
2. **Create the site** (skip if already created in the dashboard):
   ```bash
   npx -y netlify-cli sites:create --name <slug> --account-slug "rank-and-rent"
   ```
   The slug becomes `<slug>.netlify.app`. Use a real name — Netlify-generated slugs (`kaleidoscopic-dragon-...`) are unsearchable later.
3. **Link the local folder to the site:**
   ```bash
   npx -y netlify-cli link
   ```
   Pick `Rank and Rent` team, then the site. Creates `.netlify/state.json` (gitignored).
4. **Verify:** `npx -y netlify-cli status` should print the site name + team.
5. **Deploy** with the command above.

For staging/preview deploys, drop `--prod` — you get a one-off URL that doesn't touch the live site.

---

## Project Structure Conventions

```
project-root/
├── netlify.toml              # config (build, redirects, functions, headers, schedules)
├── netlify/
│   └── functions/            # serverless functions (one .js/.ts per endpoint)
│       └── my-function.js
├── public/  or  dist/  or  . # static assets / build output
└── package.json
```

Minimal `netlify.toml`:

```toml
[build]
  publish = "."
  functions = "netlify/functions"

[functions]
  node_bundler = "esbuild"

[[redirects]]
  from = "/api/*"
  to = "/.netlify/functions/:splat"
  status = 200

# [functions."my-cron"]
#   schedule = "*/15 * * * *"
```

---

## Function Types

| Type | Filename suffix | Timeout | Use for |
|------|----------------|---------|---------|
| Synchronous | `my-fn.js` | 10s (free) / 26s (pro) | API endpoints, user-facing requests |
| Background | `my-fn-background.js` | 15 min | Cron jobs, long syncs, anything >10s |
| Edge | in `netlify/edge-functions/` | 50ms CPU | Geo, A/B routing, header rewrites |
| Scheduled | any function + `schedule` in toml | matches type above | Cron-driven work |

**Always syntax-check before deploying:**
```bash
node -c netlify/functions/my-function.js
```
One syntax error fails the entire deploy.

---

## Environment Variables

Set per-site via CLI (preferred — keeps it scriptable):

```bash
npx -y netlify-cli env:set MY_KEY "value"
npx -y netlify-cli env:set MY_SECRET "value" --secret    # masked in logs
npx -y netlify-cli env:list
```

Functions read them via `process.env.MY_KEY`. Env vars are **baked at deploy time** — set them, then redeploy. They are NOT bundled into client-side JS; for that, expose them under `[build.environment]` in `netlify.toml`.

---

## Cache-Busting Client JS

Netlify's CDN caches static assets aggressively. After editing client-side JS, **bump the version query string in every HTML file that references it**:

```html
<script src="/js/app.js?v=12"></script>
```

Forgetting this is the #1 cause of "I deployed but nothing changed." If multiple HTML files reference the same script, update all of them — including any SSR functions that hardcode `<script src=>` in their HTML response (easy to miss).

---

## Deploy Workflow

1. **Read `CLAUDE.md`** in the project root for project-specific rules. Many projects override the standard command or have explicit "do NOT auto-deploy" rules.
2. Make code changes.
3. Syntax-check modified functions: `node -c netlify/functions/<file>.js`
4. Bump JS cache versions if client JS changed.
5. Update `CHANGELOG.md` if the project requires it.
6. **Ask Austin before deploying to prod.** Default behavior is never deploy automatically. Production is visible to real users — confirm first.
7. Run the deploy command. Use a 300s (5 min) timeout — function bundling can be slow on cold installs.
8. Verify the deploy URL printed at the end loads. Spot-check the changed endpoint.

---

## Common Gotchas

- **Searching sites by domain name fails** — Netlify auto-generated slugs (`kaleidoscopic-dragon-...`) have nothing to do with the live domain. Use the Site ID (UUID) for any scripted lookup, not the name. The existing real estate site is a perfect example: domain is austinmundayrealestate.com, site name is `kaleidoscopic-dragon-5efa57`.
- **Local dev doesn't run functions** — `npx serve` and similar static servers won't execute `netlify/functions/*`. Use `netlify dev` if you need functions locally. Expect JSON parse errors from `/.netlify/functions/*` when running against a plain static server.
- **`--dir=.` includes everything** — including `node_modules` if not gitignored. For static sites, prefer a `dist/` or `public/` folder.
- **Function size limit**: 50 MB unzipped, 250 MB total per deploy. Heavy deps (puppeteer, ffmpeg) will fail bundling.
- **Cold starts**: first request after idle is 1–3s slower. Background functions don't have this issue.
- **No filesystem persistence** — `/tmp` is per-invocation. Use Netlify Blobs, a Postgres DB (Neon is the default for these projects — NOT Supabase), or external storage for any state.
- **Cron is UTC** — `schedule = "0 9 * * *"` runs at 09:00 UTC, not local time. Convert manually.
- **Background functions can run concurrently** — for singleton crons, implement a mutex via Netlify Blobs (`running` flag + `startedAt` timestamp + grace window). The real estate site uses a 16-min `GUARD_MS` pattern across all background jobs.

---

## Useful CLI Commands

```bash
npx -y netlify-cli status                    # which site is linked
npx -y netlify-cli sites:list                # all sites in team
npx -y netlify-cli env:list                  # current env vars
npx -y netlify-cli functions:list            # functions detected
npx -y netlify-cli logs:function <fn-name>   # tail function logs (live)
npx -y netlify-cli deploy --prod --dir=.     # production deploy
npx -y netlify-cli deploy --dir=.            # preview deploy (returns staging URL)
npx -y netlify-cli open                      # open site dashboard in browser
npx -y netlify-cli open:admin                # open team admin
```

---

## When Things Break

- **Build fails on function bundling** → run `node -c` on every modified function. Look for missing imports, top-level `await` in CommonJS, oversized deps.
- **"Page not found" on a route that should work** → check `netlify.toml` redirects order; first match wins.
- **Function returns 502** → tail logs with `logs:function`. Usually an unhandled promise rejection or a sync function exceeding 10s (convert to `-background`).
- **Env var not picked up** → redeploy after setting it. Env vars are baked at deploy, not read live.
- **Identity/Auth widget broken (Google OAuth)** → known Netlify Identity widget bug #375. The fix on the existing site is a callback-bounce + manual JWT + localStorage fallback. Do NOT try timing-based fixes (listeners, polling, timeouts) — the widget itself fails to initialize.

---

## Project-Specific Rules to Check First

When inheriting a project, scan `CLAUDE.md` and `MEMORY.md` for:

- Explicit deploy command (some projects override the standard one — e.g. LoadHog deploys to Railway, not Netlify)
- "Ask before deploying" rules — almost every prod site Austin owns has this
- Cache-version bump checklists (which HTML files reference which JS)
- Changelog requirements (date + one-liner + session log entry)
- Rate-limit-sensitive APIs that must NEVER be called from user-facing functions (e.g. MLS Grid on the real estate site — 2 RPS hard cap, suspended 3× already)
- SSR conventions — some functions inject hardcoded `<script>` tags into rendered HTML

If `CLAUDE.md` says "ask before deploying," always ask — even if the change feels safe.
