# AGENTS.md

Guidance for coding agents working in this repository.

## Architecture

`larkstack` is a **framework** (Cargo workspace) that supervises pluggable **Apps** and ships them as a single admin-console binary. Apps come in two kinds — **Integrations** (external system → Lark bridges) and **Automations** (autonomous, time/event-triggered, in-Lark) — and plug into the host via the `App`/`Instance` trait.

Framework crates (`crates/`):

- **crates/larkstack-core** — The plug-in contract + control plane. `App` (`manifest()` + `build(config) -> Arc<dyn Instance>`) and `Instance` (`run(cancel)` + `handle_action(action, params)`); `Manifest`/`ActionSpec`/`Kind`; plus `ControlPlane`/`ControlHandle`/`Status`/`Event`/`EventStore` and the tracing→event `ControlLayer`. Apps depend only on this crate.
- **crates/larkstack** — The host (lib). `Larkstack::new().register(app).run()`: a per-app supervisor state machine, the axum admin API + SSE + embedded React UI, the SQLite event store, config load + live reload, graceful shutdown. Depends only on `larkstack-core` (never on apps).
- **crates/console** — Thin binary `larkstack-console` (~10 lines): registers the bundled apps and runs the host. One process, one deploy.

Apps (`apps/`):

- **apps/integrations/linear-bridge** (Integration) — Linear webhook → Lark notification bridge. Three-layer pipeline: `sources/` receive webhooks and normalize to a unified `Event`, `sinks/` format and deliver, middle layer (`debounce`, `dispatch`) operates on `Event` only.
- **apps/automations/meeting-digest** (Automation) — Auto-transcribe Lark/Feishu recorded meetings and post digest cards. STT via `whisper-api` or `whisper-cpp` (feature flag). Uses `larkoapi` for all Lark API surface.
- **apps/automations/standup-bot** (Automation) — Daily standup reminder bot. Scheduler + on-demand actions. Uses `larkoapi` over a WebSocket long connection.

Single workspace `Cargo.lock` at the root; members are `["crates/*", "apps/*/*"]`. Each app keeps its own `[[bin]]` so it can still run standalone (`cargo run -p linear-bridge`) via its `run()` + env-var config, but the deployed artifact is `larkstack-console`.

### The App contract

An App is a registered descriptor (`fn app() -> Arc<dyn App>`) that builds a config-bound `Instance`. The host owns the lifecycle:

- reads `[<app-name>].enabled` from the config (default **false**) → `Stopped` when off;
- when enabled: `Starting` → `App::build(full_toml)` → `Running`; a build error or `Instance::run` returning/panicking → `Errored` + exponential-backoff restart (panics are caught as `JoinError`, never left showing green);
- drives `Instance::run(cancel)` (the main loop; must honor the `CancellationToken` for cooperative shutdown) and `Instance::handle_action(name, params) -> Result<String>` (console-dispatched actions) concurrently; action results are surfaced to the event stream;
- a config change restarts **only** the apps whose own `[name]` section changed — editing one app never bounces another.

`App::build` reads its `[name]` section from the full TOML, overlaying env vars (`LINEAR_*`, `LARK_*`, …) per field, so secrets stay in the environment while ops fields are edited from the UI. Toggle an app by flipping `[name].enabled` in the config (UI Config tab → PUT → the supervisor (re)starts it).

**Frontend** lives in `crates/larkstack/web/` (React + Vite). `npm run build` emits to `crates/larkstack/web/dist/`, which `rust-embed` bakes into the host at compile time. `crates/larkstack/build.rs` writes a stub `index.html` if the frontend isn't built yet so `cargo build` always succeeds.

## Development Environment

The repo uses **[devenv](https://devenv.sh)** (`devenv.nix` + `devenv.yaml`) with **direnv** for auto-activation.
On entering the directory the dev shell auto-loads Rust stable (clippy, rustfmt, rust-analyzer) and `protoc` (required by the `larkoapi` build script).

```bash
# Prereqs: Nix (flakes enabled), direnv, devenv (`nix profile install nixpkgs#devenv`)
direnv allow            # one-time, then `cd` triggers shell auto-load
```

Without direnv, drop into the same shell via `devenv shell`.

Note: `.envrc` calls `eval "$(devenv print-dev-env)"` directly instead of `use devenv` to sidestep a SIGABRT bug in devenv 2.1.2's `direnv-export` subcommand on macOS.

## Build Commands

Workspace commands run from the repo root.

```bash
cargo fmt --all -- --check                                  # format
cargo clippy --workspace --all-targets -- -D warnings       # lint
cargo test --workspace
cargo build -p console --release                            # umbrella binary -> target/release/larkstack-console
cargo build -p linear-bridge --release                      # standalone app bin
cargo build -p meeting-digest --release
cargo build -p standup-bot --release

# Frontend (required before `cargo build -p console` for a non-stub UI)
cd crates/larkstack/web && npm install && npm run build
```

## crates/larkstack (host)

`Larkstack::new().register(app).run()`. `run()` opens the SQLite store, loads/creates `config.toml`, installs the tracing→event layer, spawns one `supervisor::supervise` task per registered app, then serves the admin API + UI.

Supervisor (`src/supervisor.rs`): the per-app state machine described under *The App contract*. Change detection and the `enabled` check parse the app's top-level config section; an exponential backoff (1s→60s) governs build-error/crash restarts.

Routes:
- `GET /api/status` — `{ "subsystems": { "<name>": { "state", "message", "updated_at" } } }`
- `GET /api/apps` — registered app manifests `{ "apps": [{ name, kind, description, actions }] }` (for generic UI rendering)
- `GET /api/events` — SSE stream of `Event { id, level, subsystem, target, message, fields, timestamp }`. Honors `Last-Event-ID` / `?since=<id>` for backfill; otherwise replays the most recent 200 events from SQLite, then streams live.
- `GET /api/config` — current TOML. `PUT /api/config` — validates by parsing, writes the file, broadcasts via `ControlPlane`'s watch channel; each supervisor sees the change and restarts only if its own section changed.
- `POST /api/actions/{app}/{action}` — fire-and-forget; body is the optional `params` JSON. `202` on dispatch, `404` unknown app, `503` app not running. The result string from `Instance::handle_action` appears in the SSE event stream.
- `GET /api/health` — `"ok"`. `GET /*` — embedded React SPA (falls back to `index.html`).

Env: `CONSOLE_PORT` (default `8080`), `CONSOLE_DATA_DIR` (default `./data`, holds `events.db` + `config.toml`), `CONSOLE_TOKEN` (required `Authorization: Bearer <token>` for `/api/*` except `/api/health`; SSE clients pass `?token=…`; unset = warn + no auth).

Shutdown: SIGINT/SIGTERM → `axum::serve(...).with_graceful_shutdown(...)`. Event log retention: SQLite keeps the most recent 10,000 events (rolling); on startup the host advances the in-memory id counter past `MAX(id)`.

Container: workspace-root `Dockerfile` (node → rust → debian:slim); `docker-compose.yml` mounts a named volume at `/data`.

## crates/console

Thin binary: `Larkstack::new().register(linear_bridge::app()).register(meeting_digest::app()).register(standup_bot::app()).run().await`. Adding an app = one `.register(...)` + a crate dep.

## apps/integrations/linear-bridge

- `src/sources/linear/` — Webhook handler (HMAC-SHA256 verification), GraphQL client for link previews
- `src/sinks/lark/` — Bot client + card types re-exported from `larkoapi`, webhook sender, interactive cards, event handler
- `src/event.rs` — Unified `Event` enum (`IssueCreated`/`Updated`, `CommentCreated`) with `Priority` normalization
- `src/debounce.rs` — In-memory debounce (tokio tasks + oneshot cancel)
- `src/dispatch.rs` — Routes events to all sinks
- `src/config.rs` — `figment` + env vars prefixed `LINEAR_`, `LARK_`; `AppState::from_toml(full_toml)`
- `src/app.rs` — `App`/`Instance` impl; `run::serve(cancel)` binds the webhook server, `actions::handle(...)` runs `ping`/`test-lark`

Routes (the app's own axum server on its configured port): `POST /webhook`, `POST /lark/event`, `GET /health`. A `Dockerfile` lives inside this crate for standalone Railway deploys.

## apps/automations/meeting-digest

Pipeline: VC `meeting_ended` / `recording_ready` event → fetch recording → STT → summarize → interactive card + optional Lark Doc attachment. Action: `process-meeting` (params: `meeting_id`, optional `owner`/`url`).

Key modules:
- `events.rs` — Lark VC event subscription dispatch
- `pipeline.rs` — End-to-end orchestration
- `stt/{whisper_api,whisper_cpp}.rs` — Selected via feature flag (`whisper-api` default, `whisper-cpp` opt-in)
- `lark/{card,docs}.rs` — Digest card builder + Lark Docs attachment
- `app.rs` — `App`/`Instance` impl; `run::serve_ws(cancel)`, `actions::handle(...)`

Config via `figment` + env vars; see `apps/automations/meeting-digest/README.md`.

## apps/automations/standup-bot

Daily standup runner: WebSocket command bot + scheduler (Asia/Shanghai). Actions: `announce | ensure | remind | urgent | urgent-user | check` (accept optional `today | tomorrow | YYYY-MM-DD`; `urgent-user` also needs `open_id`).

Modules:
- `scheduler.rs` — Cron-style triggers for announce/remind/urgent
- `flow.rs` — Standup doc creation, sharing, escalation
- `commands.rs` — `WsEventHandler` impl for chat command parsing (`@-mention` detection)
- `templates.rs` — `askama` template rendering for chat replies
- `app.rs` — `App`/`Instance` impl; `run::serve_with_bot(cancel)`, `actions::handle(...)`

Required env: `LARK_APP_ID`, `LARK_APP_SECRET`, `STANDUP_CHAT_ID`, `STANDUP_FOLDER_TOKEN`, `STANDUP_ENABLED=true` (scheduler). Optional: `LARK_BASE_URL` (default `https://open.larksuite.com`). Note the `[standup-bot].enabled` host toggle is distinct from `[standup-bot.standup].enabled` (scheduler auto-fire).

The repo-relative `.cargo/config.toml` carries a hard-coded musl cross-compile linker path for the original author's machine; adjust for your toolchain.

## Lark API Patterns

All apps target Lark (international: `open.larksuite.com`, China: `open.feishu.cn`). Base URL is configurable; most Lark surface comes from the `larkoapi` crate.

- **Token caching**: Tenant access tokens are cached with a 5-minute expiry buffer.
- **Card format**: JSON 1.0 (`header` + `elements` at top level). Use `column_set` for multi-column layout, `action` for button rows. Buttons cannot be nested inside `column` elements.
- **WebSocket protocol**: POST `/callback/ws/endpoint` with `AppID`/`AppSecret` → get WSS URL → protobuf binary frames. Card action callbacks arrive as `event_type: "card.action.trigger"` with `frame_type: "event"` (not `"card"`).
- **Card callback ACK**: Response in ACK frame payload as `{"code": 200, "data": "<base64 of response JSON>"}`. Response JSON: `{"card": {"type": "raw", "data": {<card JSON>}}}`.
