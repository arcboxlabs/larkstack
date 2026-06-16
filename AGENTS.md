# AGENTS.md

Guidance for coding agents working in this repository.

## Architecture

`larkstack` is a **framework** (Cargo workspace) that supervises pluggable **Apps** and ships them as a single admin-console binary. Apps come in two kinds — **Integrations** (external system → Lark bridges) and **Automations** (autonomous, time/event-triggered, in-Lark) — and plug into the host via the `App`/`Instance` trait.

Framework crates (`crates/`):

- **crates/larkstack-core** — The plug-in contract + control plane. `App` (`manifest()` + `build(config) -> Arc<dyn Instance>` + optional `migrations()` + `routes()`) and `Instance` (`run(cancel)` + `handle_action(action, params)`); `Manifest`/`ActionSpec`/`Kind`; the `LarkApp`/`LarkRegistry` credential registry (`[lark-apps.<name>]`); the per-App persistence services handed in via `AppServices` (`StateStore` KV, `MetricsSink`, and the shared relational `db`); the `db` module — a shared-SQLite App-table store with a migration runner that **enforces a per-App `"<app>_"` table-name prefix** (see *App-owned tables* below); plus `ControlPlane`/`ControlHandle`/`Status`/`Event`/`EventStore` and the tracing→event `ControlLayer`. Apps depend only on this crate (plus `lark-kit` for the Lark integrations).
- **crates/larkstack** — The host (lib). `Larkstack::new().register(app).run()`: a per-app supervisor state machine, the axum admin API (in `src/routes/`, OpenAPI-documented via `utoipa`) + SSE + embedded React UI, the SQLite event store, config load + live reload, graceful shutdown. Depends only on `larkstack-core` (never on apps).
- **crates/console** — Thin binary `larkstack-console` (~12 lines): registers the bundled apps and runs the host. One process, one deploy.
- **crates/lark-kit** — Shared toolkit for the Lark **Integration** apps (not the framework). Source-agnostic: the Lark card builders (`card::{card, message, md_div, link_button}`), the group-webhook sender + DM bot, the per-app `LarkConfig` (+ `[lark-apps]` resolution helpers), the `slot` state cell (`StateSlot`/`SlotGuard`/`Live`) that backs host-mounted ingress routers, the event-callback scaffold (`event::classify`: AES-256-CBC decrypt, challenge, token check, `url.preview.get` → `Callback`), and `verify_hmac_sha256`/`truncate`.

Apps (`apps/`):

- **apps/integrations/linear** (Integration) — Linear webhook → Lark notifications + issue link previews. `POST /webhooks/linear/webhook` (debounced issue/comment cards), `POST /webhooks/linear/lark/event` (preview).
- **apps/integrations/github** (Integration) — GitHub webhook → Lark notifications. `POST /webhooks/github/webhook`; octocrab native models; PR/issue/CI/security-alert cards + review-request DMs.
- **apps/integrations/x** (Integration) — X (Twitter) link previews. `POST /webhooks/x/lark/event`; fetches tweet data (`XClient`) and replies with a preview card. Preview-only — no notifications.
- **apps/automations/minutes** (Automation) — Auto-transcribe Lark/Feishu recorded meetings and post digest cards. STT via `whisper-api` or `whisper-cpp` (feature flag). Uses `larkoapi` for all Lark API surface.
- **apps/automations/standup** (Automation) — Daily standup reminder bot. Scheduler + on-demand actions. Uses `larkoapi` over a WebSocket long connection.

The three integrations each own their source + cards and share `lark-kit`; there is **no** cross-app `Event` enum. Each contributes its inbound router via `App::ingress_routes`, which the host mounts on the console port under `/webhooks/<app>/` — so there are no per-app ports.

Single workspace `Cargo.lock` at the root; members are `["crates/*", "apps/*/*"]`. The deployed artifact is `larkstack-console`; the integration apps (linear/github/x) are libraries with no `[[bin]]` — the console is their only entry point. The automations (minutes/standup) keep a `[[bin]]` for standalone/CLI use.

### The App contract

An App is a registered descriptor (`fn app() -> Arc<dyn App>`) that builds a config-bound `Instance`. The host owns the lifecycle:

- reads `[<app-name>].enabled` from the config (default **false**) → `Stopped` when off;
- when enabled: `Starting` → `App::build(full_toml)` → `Running`; a build error or `Instance::run` returning/panicking → `Errored` + exponential-backoff restart (panics are caught as `JoinError`, never left showing green);
- drives `Instance::run(cancel)` (the main loop; must honor the `CancellationToken` for cooperative shutdown) and `Instance::handle_action(name, params) -> Result<String>` (console-dispatched actions) concurrently; action results are surfaced to the event stream;
- a config change restarts **only** the apps affected: an app's *change key* is its own `[name]` section **plus** the `[lark-apps.<ref>]` entry it binds to, so editing one app — or the shared Lark credentials it references — never bounces an unrelated app.

`App::build` reads its `[name]` section from the full TOML, overlaying env vars (`LINEAR_*`, `LARK_*`, …) per field, so secrets stay in the environment while ops fields are edited from the UI. Toggle an app by flipping `[name].enabled` in the config (UI Config tab → PUT → the supervisor (re)starts it).

**App-owned tables.** Beyond the `StateStore` KV, an App can own relational tables in the shared App database (`<data_dir>/apps.db`, a sea-orm/sqlx SQLite handle on `AppServices.db`). An App declares schema via `App::migrations() -> Vec<Box<dyn MigrationTrait>>` (sea-orm migrations); the host runs them at startup — **before** the app is enabled, so the tables exist for admin use first — through `larkstack_core::db::run_migrations`, the framework's own runner (not sea-orm's `Migrator`). The runner tracks applied migrations per-App in one `_larkstack_migrations(app, name)` table and runs each migration in a transaction that is **rolled back unless every table it creates/drops is namespaced `"<app>_"`** — so the prefix is enforced, not merely conventional (caveat: cross-App *alters* aren't detectable and aren't blocked). A migration failure leaves just that app Errored, not the whole console.

**App-contributed routes.** An App can expose admin endpoints via `App::routes(&services) -> Option<Router>` (mounted at `/api/apps/<name>/`, **behind** the session gate; first consumer: linear's `user_map`, Linear→Lark email overrides) and public inbound endpoints via `App::ingress_routes(&services) -> Option<Router>` (mounted at `/webhooks/<name>/`, **outside** the gate — webhooks authenticate with their own HMAC/token). Both are self-stated and absent from the OpenAPI spec. Ingress routers are mounted **once** at startup, but an integration's `AppState` is rebuilt on every config reload — so the app publishes its live state into a `lark_kit::StateSlot` held on the (process-lifetime) App descriptor: the running `Instance` stores into the slot on `run` and clears it via a `SlotGuard` on stop, and the handler reads the current `AppState` through the `lark_kit::Live` extractor (or returns `503` while the app is down/reloading).

**Lark-app registry.** Lark credentials live once under `[lark-apps.<name>] = { app_id, app_secret, base_url }`; an app binds to one with `lark_app = "<name>"` in its own section (resolved in each app's `from_toml`, before the inline `[<app>].lark` / env overlay). Onboard/manage entries from the console's **Lark Apps** tab, which live-tests the credentials (mints a `tenant_access_token`) before saving. Credentials are stored plaintext in `config.toml` (gated behind the console's Lark-OAuth session); the registry GET redacts `app_secret`.

**Frontend** lives in `dashboard/` at the repo root (React + Vite, **pnpm**). `pnpm build` emits to `dashboard/dist/`, which `rust-embed` bakes into the host at compile time (the host crate embeds `../../dashboard/dist/`). `crates/larkstack/build.rs` writes a stub `index.html` if the frontend isn't built yet so `cargo build` always succeeds.

## Development Environment

The repo uses **[devenv](https://devenv.sh)** (`devenv.nix` + `devenv.yaml`) with **direnv** for auto-activation.
On entering the directory the dev shell auto-loads Rust stable (clippy, rustfmt, rust-analyzer), `protoc` (required by the `larkoapi` build script), and the frontend toolchain (Node.js + `pnpm`) for `dashboard/`.

```bash
# Prereqs: Nix (flakes enabled), direnv, devenv (`nix profile install nixpkgs#devenv`)
direnv allow            # one-time, then `cd` triggers shell auto-load
```

Without direnv, drop into the same shell via `devenv shell`.

Note: `.envrc` calls `eval "$(devenv print-dev-env)"` directly instead of `use devenv` to sidestep a SIGABRT bug in devenv 2.1.2's `direnv-export` subcommand on macOS.

**Docs convention:** this file is `AGENTS.md`; `CLAUDE.md` is a symlink to it (same for each crate) — edit `AGENTS.md`, since writing through the symlink fails. Per-app specifics live in `apps/<…>/AGENTS.md` (e.g. `apps/integrations/linear/AGENTS.md`); read that crate's file before working inside it.

## Build Commands

Workspace commands run from the repo root.

```bash
cargo fmt --all -- --check                                  # format
cargo clippy --workspace --all-targets -- -D warnings       # lint
cargo test --workspace
cargo test -p larkstack-core db::tests                      # one crate + filter (run a single test / module)
cargo build -p console --release                            # umbrella binary -> target/release/larkstack-console
cargo build -p standup --release                            # automations still ship a [[bin]] (minutes likewise); integrations are libs

# Run the console locally (debug); state under ./data (CONSOLE_DATA_DIR), UI on :8080 (CONSOLE_PORT)
cargo run -p console

# Frontend: embedded build — required before `cargo build -p console` for a non-stub UI
cd dashboard && pnpm install && pnpm build
# Frontend dev loop: Vite dev server (hot reload) proxies /api + /auth to a console on :8080
cd dashboard && pnpm dev        # run alongside `cargo run -p console`
```

## crates/larkstack (host)

`Larkstack::new().register(app).run()`. `run()` opens the SQLite store, loads/creates `config.toml`, installs the tracing→event layer, spawns one `supervisor::supervise` task per registered app, then serves the admin API + UI.

The HTTP surface lives in `src/routes/` (one module per group: `status`, `config`, `events`, `actions`, `lark_apps`, `oauth`, plus shared `OkResponse`/`ErrorResponse`/`ApiError` in `mod.rs`). `routes::build(state)` assembles the whole router through `utoipa-axum`'s `OpenApiRouter`, so the OpenAPI spec is collected from the very route definitions that are served — it can't drift. Request/response bodies are typed `ToSchema` structs; the three endpoints that wrap `larkstack-core` types (`status`/`apps`) map core → wire structs explicitly so `larkstack-core` never has to depend on `utoipa`.

Supervisor (`src/supervisor.rs`): the per-app state machine described under *The App contract*. The `enabled` check and change detection use a `ChangeKey` = the app's top-level section + the `[lark-apps.<ref>]` it binds to; an exponential backoff (1s→60s) governs build-error/crash restarts.

Routes:
- `GET /api/status` — `{ "subsystems": { "<name>": { "state", "message", "updated_at" } } }`
- `GET /api/apps` — registered app manifests `{ "apps": [{ name, kind, description, actions }] }` (for generic UI rendering)
- `GET /api/events` — SSE stream of `Event { id, level, subsystem, target, message, fields, timestamp }`. Honors `Last-Event-ID` / `?since=<id>` for backfill; otherwise replays the most recent 200 events from SQLite, then streams live.
- `GET /api/config` — current TOML. `PUT /api/config` — validates by parsing, writes the file, broadcasts via `ControlPlane`'s watch channel; each supervisor sees the change and restarts only if its own change key changed.
- `GET /api/lark-apps` — registered Lark apps `{ "lark_apps": [{ name, app_id, base_url, has_secret }] }` (`app_secret` redacted). `POST /api/lark-apps` — body `{ name, app_id, app_secret, base_url? }`; **live-tests** the credentials and, only if valid, upserts `[lark-apps.<name>]` via `toml_edit` (comments preserved) + broadcasts. `400` if the test fails (nothing saved). `POST /api/lark-apps/test` — dry-run the same check without saving (`200 {ok, expire|error}`). `DELETE /api/lark-apps/{name}` — remove an entry (`404` if absent).
- `POST /api/actions/{app}/{action}` — fire-and-forget; body is the optional `params` JSON. `202` on dispatch, `404` unknown app, `503` app not running. The result string from `Instance::handle_action` appears in the SSE event stream.
- `/api/apps/{app}/*` — App-contributed admin routes (`App::routes`), mounted per registered app behind the session gate; shape is App-defined (e.g. linear's `GET/POST /api/apps/linear/user-map`, `DELETE /api/apps/linear/user-map/{linear_email}`). Not part of the OpenAPI spec.
- `/webhooks/{app}/*` — App-contributed public inbound routes (`App::ingress_routes`), mounted per registered app **outside** the session gate (callers authenticate with their own HMAC/token, not a console session): `POST /webhooks/linear/webhook` + `/webhooks/linear/lark/event`, `POST /webhooks/github/webhook`, `POST /webhooks/x/lark/event`. Not part of the OpenAPI spec.
- **Auth (Lark OAuth, `src/routes/oauth.rs`)** — `GET /auth/login` (mint state + PKCE, redirect to Lark's `accounts.*/open-apis/authen/v1/authorize`), `GET /auth/callback` (verify state, exchange the code at `open.*/open-apis/authen/v2/oauth/token`, fetch `user_info`, check the `admins` allowlist, set a signed session cookie), `POST /auth/logout` (clear it), `GET /auth/me` (ungated — `{ auth_required, authenticated, user? }`; the UI uses it to decide whether to show the login screen). `/api/*` (except `/api/health`, `/api/openapi.json`, `/api/docs`) is gated by the signed-cookie session, resolved per-request from the live config, and stays OPEN while `[console].lark_app` is unbound.
- `GET /api/openapi.json` — the OpenAPI 3.1 spec, generated by `utoipa` from the route definitions. `GET /api/docs` — a Scalar API explorer over it (the page loads Scalar's JS from a CDN). Both ungated: the spec is shapes only, no data.
- `GET /api/health` — `"ok"`. `GET /*` — embedded React SPA (falls back to `index.html`).

Env: `CONSOLE_PORT` (default `8080`), `CONSOLE_DATA_DIR` (default `./data`, holds `events.db` + `config.toml` + `state.db`/`metrics.db` + `apps.db` (App-owned tables) + the auto-generated `session.key`), `CONSOLE_SECRET` (optional; derives the cookie signing key so sessions survive restarts — else a random key is persisted to `session.key`). Console sign-in is **Lark OAuth**, configured under `[console]` (`lark_app` binds a `[lark-apps.<name>]`; `admins` is the email allowlist, empty = any tenant user; optional `redirect_uri`/`scope`). The allowlist matches on the Lark `user_info` `email`/`enterprise_email`, which are only returned when their contact scopes are requested — so when `admins` is non-empty and `[console].scope` is unset, the authorize request defaults to the full user-info identity set (`contact:user.email|employee|employee_id|phone:readonly`); an explicit `scope` overrides it (empty = none). Every requested scope must be granted on the Lark app's Permission Management page or the authorize page fails with error `20027`.

Shutdown: SIGINT/SIGTERM → `axum::serve(...).with_graceful_shutdown(...)`. Event log retention: SQLite keeps the most recent 10,000 events (rolling); on startup the host advances the in-memory id counter past `MAX(id)`.

Container: workspace-root `Dockerfile` (node → rust → debian:slim); `docker-compose.yml` mounts a named volume at `/data`.

## crates/console

Thin binary: `Larkstack::new().register(linear::app()).register(github::app()).register(x::app()).register(minutes::app()).register(standup::app()).run().await`. Adding an app = one `.register(...)` + a crate dep.

## apps/integrations/{linear, github, x} + crates/lark-kit

Each integration is its own App crate (own source + cards + `AppState`), all building on **`crates/lark-kit`** (the Lark sink/config/crypto/slot toolkit; see *Architecture*). They share no `Event` enum — each builds cards directly from its source models and posts via `lark_kit::send_lark_card`. Every app's `from_toml` reads its `[<app>]` section, resolves an optional `lark_app = "<name>"` against `[lark-apps]` (via `LarkConfig::apply_lark_app`), then overlays `[<app>.lark]`. Each app's `routes`/`run` module exposes `ingress_router(slot) -> Router`, which `App::ingress_routes` hands to the host to mount at `/webhooks/<app>/`; handlers read live state via the `lark_kit::Live` extractor.

**linear** (`apps/integrations/linear`) is organized by boundary: `routes/` is the inbound HTTP surface (`routes::webhook` + `routes::preview`), `domain/` the normalized core (`IssueNotification`/`Priority` + `debounce`), `source/` the Linear adapter (`payload` webhook types, `changes` detection, `api` GraphQL client), `lark/` the Lark adapter (`cards` + `notify`). Flow: `POST /webhooks/linear/webhook` (HMAC `linear-signature`) → `source::payload` normalizes to `domain::IssueNotification` → `domain::debounce` (issues) → `lark::cards::issue_card`/`comment_card` → group webhook + assignee DM. `POST /webhooks/linear/lark/event` → `routes::preview` (via `lark_kit::event::classify`) → `source::api` GraphQL fetch → `lark::cards::preview_card`. The `api` GraphQL bindings are generated by `graphql_client` from the committed `graphql/schema.graphql` (a pinned lock; refresh from Linear's SDK with the `update-linear-schema` devenv script). Env: `LINEAR_*`, `LARK_*`; `[linear].debounce_delay_ms` tunes the issue-update coalescing window.

**github** (`apps/integrations/github`): `POST /webhooks/github/webhook` (`X-Hub-Signature-256`, repo whitelist) → octocrab native `WebhookEvent` → `cards::*` (PR opened/review-requested/merged, alert-labeled issues, failed CI, secret-scanning, critical/high Dependabot) → group webhook + review-request DM (`user_map`: GitHub login → Lark email). `build()` errors if `webhook_secret` is empty. Env: `GITHUB_*`, `LARK_*`.

**x** (`apps/integrations/x`): `POST /webhooks/x/lark/event` (`lark_kit::event::classify` handles decrypt/token) → `source::XClient` fetches the tweet (fxtwitter → X API v2 → oEmbed; `X_BEARER_TOKEN` optional) → `cards::x_preview`. Preview-only, no notifications.

Each app contributes its router via `App::ingress_routes`; the host mounts them on the console port under `/webhooks/<app>/` (no per-app ports).

## apps/automations/minutes

Pipeline: VC `meeting_ended` / `recording_ready` event → fetch recording → STT → summarize → interactive card + optional Lark Doc attachment. Action: `process-meeting` (params: `meeting_id`, optional `owner`/`url`).

Key modules:
- `events.rs` — Lark VC event subscription dispatch
- `pipeline.rs` — End-to-end orchestration
- `stt/{whisper_api,whisper_cpp}.rs` — Selected via feature flag (`whisper-api` default, `whisper-cpp` opt-in)
- `lark/{card,docs}.rs` — Digest card builder + Lark Docs attachment
- `app.rs` — `App`/`Instance` impl; `run::serve_ws(cancel)`, `actions::handle(...)`

Config via `figment` + env vars; see `apps/automations/minutes/README.md`.

## apps/automations/standup

Daily standup runner: WebSocket command bot + scheduler. Actions: `announce | ensure | remind | urgent | urgent-user | check` (accept optional `today | tomorrow | YYYY-MM-DD`; `urgent-user` also needs `open_id`).

Organized by architectural role (ports-and-adapters, like the linear app). The five operations (`ensure`/`announce`/`remind`/`urgent_one`/`check`) live once in `flow.rs` — the domain core — and every inbound surface translates its trigger into a `flow` call; the Lark API is reached only through the `lark/` adapter:
- `flow.rs` — Domain: the high-level standup operations. Composes `lark::doc` + `lark::card`; the single source of orchestration. Takes the live `&Settings`.
- `lark/` — Outbound adapter (the only code that talks to Lark).
  - `lark/doc.rs` — The standup doc itself: create the per-day Docx, seed the member table, read it back to detect who hasn't filled their cells. Title/headers/column-widths come from `Settings`.
  - `lark/card.rs` — Announce + reminder card builders; title/body rendered from the `Settings` minijinja templates (only the card colors are fixed).
- `settings/` — Admin-tunable runtime behavior, stored as **one JSON blob in the per-App `StateStore` KV** (namespace `standup`, key `settings`) — a singleton config, never queried, so the KV fits better than a relational table (no sea-orm). `Settings`/`Default`/`load`/`save`; tolerant decode (`#[serde(default)]`, bad values → defaults). Holds the schedule (per-job time + enable + IANA timezone), the doc wording (title/headers/column-widths), and the six minijinja message templates. `routes.rs` serves admin `GET/PUT /api/apps/standup/settings` (mounted via `App::routes` using `services.state`). Edited live from the console's **Standup** tab — no restart; the scheduler/bot reload each pass.
- `template.rs` — Runtime minijinja rendering (`render(tpl, ctx)`); templates are admin-editable strings, so they're evaluated at runtime, not compiled (replaced askama).
- `trigger/` — Inbound surfaces that drive `flow` (the standalone CLI in `main.rs` is the fourth). Each loads `settings` fresh so console edits apply at once.
  - `trigger/scheduler.rs` — Autonomous timer; the four jobs read time/enable/timezone from `settings` each pass (DST-safe), honor `cancel`.
  - `trigger/commands.rs` — `WsEventHandler` impl for chat command parsing (`@-mention` detection).
  - `trigger/actions.rs` — Console action dispatch (`handle(action, params)`).
- `runtime/` — Bootstrap + console-host integration.
  - `runtime/app.rs` — `App`/`Instance` impl registered by the console; `routes()` mounts the settings router; the instance holds the `StateStore`.
  - `runtime/run.rs` — `build_bot` + `serve_with_bot` (WS bot + scheduler wiring); shared by the host instance and the standalone binary.
- `config.rs` — Secrets/bindings only (`[standup]`: `chat_id`, `folder_token`, `lark_app`, `enabled`). `date.rs` — `today()`/`tomorrow()`/`resolve()`, each taking the configured timezone.

Config split (like linear): `config.toml` carries only secrets/bindings; **behavioral knobs + templates live in the `StateStore` settings blob**, edited live from the console's **Standup** tab. The standalone bin opens its own `StateStore` (`<CONSOLE_DATA_DIR>/state.db`) so the CLI shares the same settings.

Required env: `LARK_APP_ID`, `LARK_APP_SECRET`, `STANDUP_CHAT_ID`, `STANDUP_FOLDER_TOKEN`, `STANDUP_ENABLED=true` (scheduler master switch). Optional: `LARK_BASE_URL` (default `https://open.larksuite.com`). Note the `[standup].enabled` host toggle is distinct from `[standup.standup].enabled` (scheduler auto-fire); per-job toggles + times are in the settings blob.

The repo-relative `.cargo/config.toml` carries a hard-coded musl cross-compile linker path for the original author's machine; adjust for your toolchain.

## Lark API Patterns

All apps target Lark (international: `open.larksuite.com`, China: `open.feishu.cn`). Base URL is configurable; most Lark surface comes from the `larkoapi` crate.

**Rule — foundational Lark API changes go upstream to `larkoapi`.** When a basic Lark endpoint is missing, broken, or needs new behavior, fix it in the `larkoapi` crate (then bump the dependency here) — do **not** add a local wrapper or hand-roll the HTTP/protobuf call in this repo. `larkoapi` is the single source of raw Lark API surface; app and `lark-kit` code build on it, never around it. (This is about the underlying API client itself; Lark-flavored helpers that compose it — card builders, the webhook sender, the event-callback scaffold — still belong in `lark-kit`.)

- **Token caching**: Tenant access tokens are cached with a 5-minute expiry buffer.
- **Card format**: JSON 1.0 (`header` + `elements` at top level). Use `column_set` for multi-column layout, `action` for button rows. Buttons cannot be nested inside `column` elements.
- **WebSocket protocol**: POST `/callback/ws/endpoint` with `AppID`/`AppSecret` → get WSS URL → protobuf binary frames. Card action callbacks arrive as `event_type: "card.action.trigger"` with `frame_type: "event"` (not `"card"`).
- **Card callback ACK**: Response in ACK frame payload as `{"code": 200, "data": "<base64 of response JSON>"}`. Response JSON: `{"card": {"type": "raw", "data": {<card JSON>}}}`.
