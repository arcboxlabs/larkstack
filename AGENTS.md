# AGENTS.md

Guidance for coding agents working in this repository.

## Architecture

`larkstack` is a Cargo workspace shipping a single admin console binary that supervises three Lark/Feishu subsystems:

- **crates/console** — Umbrella binary `larkstack-console`. Spawns each subsystem as a tokio task, serves a React Web UI + admin API (axum). One process, one deploy.
- **crates/control** — Shared types (`ControlPlane`, `ControlHandle`, `Status`). Each subsystem receives a `ControlHandle` to report status/events back to the console.
- **crates/linear-bridge** (Rust) — Linear webhook → Lark notification bridge. Three-layer pipeline: `sources/` receive webhooks and normalize to a unified `Event`, `sinks/` format and deliver to channels, middle layer (`debounce`, `dispatch`) operates on `Event` only. Exposes `pub async fn run(state, handle)` for embedding into console; still has its own `[[bin]]` for standalone use (incl. CF Workers via the `cf-worker` feature).
- **crates/meeting-digest** (Rust) — Auto-transcribe Lark/Feishu recorded meetings and post digest cards. STT via `whisper-api` or `whisper-cpp` (feature flag). Uses `larkoapi` for all Lark API surface (meeting metadata, minute media, docx/drive, IM cards).
- **crates/standup-bot** (Rust) — Daily standup reminder bot. Scheduler + on-demand CLI subcommands (`announce`, `ensure`, `remind`, `urgent`, `check`). Uses `larkoapi` over a WebSocket long connection.

Single workspace `Cargo.lock` at the root. Each subsystem keeps its `[[bin]]` so it can still run standalone (`cargo run -p linear-bridge`), but the deployed artifact is `larkstack-console` which bundles them all.

**Frontend** lives in `crates/console/web/` (React + Vite). `npm run build` emits to `crates/console/web/dist/`, which `rust-embed` bakes into the console binary at compile time. `crates/console/build.rs` writes a stub `index.html` if the frontend hasn't been built yet so `cargo build` always succeeds.

## Development Environment

The repo uses **[devenv](https://devenv.sh)** (`devenv.nix` + `devenv.yaml`) with **direnv** for auto-activation.
On entering the directory the dev shell auto-loads Rust stable (with `wasm32-unknown-unknown` target, clippy, rustfmt, rust-analyzer) and `protoc` (required by the `larkoapi` build script).

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
cargo build -p linear-bridge --release                      # standalone bin
cargo build -p meeting-digest --release
cargo build -p standup-bot --release

# Frontend (required before `cargo build -p console` for a non-stub UI)
cd crates/console/web && npm install && npm run build

# CF Worker (linear-bridge only — the console bundle can't target Workers)
cd crates/linear-bridge && cargo check --no-default-features --features cf-worker --lib
cd crates/linear-bridge && worker-build --release
```

The mutually-exclusive `native` / `cf-worker` features in `linear-bridge` mean `cargo clippy --all-features` will hit the `compile_error!` guard. Use the workspace clippy command above (default features per crate) instead.

## crates/linear-bridge

Dual deployment: native (Tokio, default feature `native`) or Cloudflare Workers (feature `cf-worker`). Mutually exclusive — compile error if both are enabled.

- `src/sources/linear/` — Webhook handler (HMAC-SHA256 verification), GraphQL client for link previews
- `src/sinks/lark/` — Bot client (tenant token caching), webhook sender, interactive cards, event handler
- `src/event.rs` — Unified `Event` enum (`IssueCreated`/`Updated`, `CommentCreated`) with `Priority` normalization
- `src/debounce.rs` — Native in-memory debounce (tokio tasks + oneshot cancel); `debounce_do.rs` for CF Durable Objects
- `src/dispatch.rs` — Routes events to all sinks
- `src/config.rs` — `figment` + env vars prefixed `LINEAR_`, `LARK_`

Routes: `POST /webhook`, `POST /lark/event`, `GET /health`. `Dockerfile` and `wrangler.toml` live inside this crate — standalone Railway/CF deploys target `crates/linear-bridge/`. The console bundle exposes the same routes (it embeds `linear_bridge::run`).

## crates/console

Single-binary supervisor. `src/main.rs` spawns each subsystem's `run()` as a tokio task with its own `ControlHandle`. `src/assets.rs` serves the embedded React app via `rust-embed`.

Routes:
- `GET /api/status` — `{ "subsystems": { "<name>": { "state", "message", "updated_at" } } }`
- `GET /api/health` — `"ok"`
- `GET /*` — embedded React SPA (falls back to `index.html`)

Env: `CONSOLE_PORT` (default `8080`) for the console listener. Subsystem env vars (`LINEAR_*`, `LARK_*`, etc.) are read by each subsystem's own config loader, same as in standalone mode.

Phase status: only `linear-bridge` is currently wired in (Phase 2). `meeting-digest` and `standup-bot` ingestion comes in later phases (event bus / SQLite / actions / config reload).

## crates/meeting-digest

Pipeline: VC `meeting_ended` / `recording_ready` event → fetch recording → STT → summarize → interactive card + optional Lark Doc attachment.

Key modules:
- `events.rs` — Lark VC event subscription dispatch
- `pipeline.rs` — End-to-end orchestration
- `stt/{whisper_api,whisper_cpp}.rs` — Selected via feature flag (`whisper-api` default, `whisper-cpp` opt-in)
- `lark/{card,docs}.rs` — Digest card builder + Lark Docs attachment

Config via `figment` + env vars; see `crates/meeting-digest/README.md`.

## crates/standup-bot

Daily standup runner with two modes:
- `run` (default): WebSocket bot + scheduler (Asia/Shanghai timezone) for daily announcements and reminders
- One-shot subcommands: `announce | ensure | remind | urgent | urgent-user | check` — accept optional `today | tomorrow | YYYY-MM-DD`

Modules:
- `scheduler.rs` — Cron-style triggers for announce/remind/urgent
- `flow.rs` — Standup doc creation, sharing, escalation
- `commands.rs` — `WsEventHandler` impl for chat command parsing (`@-mention` detection)
- `templates.rs` — `askama` template rendering for chat replies

Required env: `LARK_APP_ID`, `LARK_APP_SECRET`, `STANDUP_CHAT_ID`, `STANDUP_FOLDER_TOKEN`, `STANDUP_ENABLED=true` (scheduler only). Optional: `LARK_BASE_URL` (default `https://open.larksuite.com`).

The repo-relative `.cargo/config.toml` in `standup-bot` carries a hard-coded musl cross-compile linker path for the original author's machine; adjust for your toolchain.

## Lark API Patterns

All `crates/*` projects target Lark (international: `open.larksuite.com`, China: `open.feishu.cn`). Base URL is configurable.

- **Token caching**: Tenant access tokens are cached with a 5-minute expiry buffer.
- **Card format**: JSON 1.0 (`header` + `elements` at top level). Use `column_set` for multi-column layout, `action` for button rows. Buttons cannot be nested inside `column` elements.
- **WebSocket protocol**: POST `/callback/ws/endpoint` with `AppID`/`AppSecret` → get WSS URL → protobuf binary frames. Card action callbacks arrive as `event_type: "card.action.trigger"` with `frame_type: "event"` (not `"card"`).
- **Card callback ACK**: Response in ACK frame payload as `{"code": 200, "data": "<base64 of response JSON>"}`. Response JSON: `{"card": {"type": "raw", "data": {<card JSON>}}}`.
