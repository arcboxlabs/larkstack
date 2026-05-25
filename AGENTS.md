# AGENTS.md

Guidance for coding agents working in this repository.

## Architecture

Polyglot repo with one Rust binary and a `crates/` directory of independent Lark/Feishu utility crates:

1. **larkstack** (root crate, Rust) — Linear webhook → Lark notification bridge. Three-layer pipeline: `sources/` receive webhooks and normalize to a unified `Event`, `sinks/` format and deliver to channels, middle layer (`debounce`, `dispatch`) operates on `Event` only.
2. **crates/meeting-digest** (Rust) — Auto-transcribe Lark/Feishu recorded meetings and post digest cards. STT via `whisper-api` or `whisper-cpp` (feature flag). Uses `larkoapi` for all Lark API surface (meeting metadata, minute media, docx/drive, IM cards).
3. **crates/standup-bot** (Rust) — Daily standup reminder bot. Scheduler + on-demand CLI subcommands (`announce`, `ensure`, `remind`, `urgent`, `check`). Uses `larkoapi` over a WebSocket long connection.

Each `crates/*` member is an independent Cargo project (own `Cargo.lock`, gitignored) and is not part of any Cargo workspace.

## Development Environment

The repo ships a Nix flake (`flake.nix`) + `.envrc` driving **direnv + nix-direnv**.
On entering the directory the dev shell auto-loads Rust stable (with `wasm32-unknown-unknown` target, clippy, rustfmt, rust-analyzer) and `protoc` (required by the `larkoapi` build script). One-time setup:

```bash
# Prereqs: Nix (with flakes), direnv, nix-direnv (`nix profile install nixpkgs#nix-direnv`)
direnv allow            # in repo root, then `cd` triggers shell auto-load
```

If you prefer not to use direnv, `nix develop` enters the same shell ad hoc.

## Build Commands

```bash
# larkstack (root)
cargo build --release                                       # native
cargo fmt --all -- --check                                  # format
cargo clippy --all-targets --all-features -- -D warnings    # lint
cargo test
cargo check --no-default-features --features cf-worker --lib  # cf-worker

# Cloudflare Workers full build
cargo install worker-build && worker-build --release

# meeting-digest
cd crates/meeting-digest && cargo build --release

# standup-bot
cd crates/standup-bot && cargo build --release
```

## larkstack (root crate)

Dual deployment: native (Tokio, default feature `native`) or Cloudflare Workers (feature `cf-worker`). Mutually exclusive — compile error if both are enabled.

- `src/sources/linear/` — Webhook handler (HMAC-SHA256 verification), GraphQL client for link previews
- `src/sinks/lark/` — Bot client (tenant token caching), webhook sender, interactive cards, event handler
- `src/event.rs` — Unified `Event` enum (`IssueCreated`/`Updated`, `CommentCreated`) with `Priority` normalization
- `src/debounce.rs` — Native in-memory debounce (tokio tasks + oneshot cancel); `debounce_do.rs` for CF Durable Objects
- `src/dispatch.rs` — Routes events to all sinks
- `src/config.rs` — `figment` + env vars prefixed `LINEAR_`, `LARK_`

Routes: `POST /webhook`, `POST /lark/event`, `GET /health`.

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

Both larkstack and the `crates/*` projects target Lark (international: `open.larksuite.com`, China: `open.feishu.cn`). Base URL is configurable.

- **Token caching**: Tenant access tokens are cached with a 5-minute expiry buffer.
- **Card format**: JSON 1.0 (`header` + `elements` at top level). Use `column_set` for multi-column layout, `action` for button rows. Buttons cannot be nested inside `column` elements.
- **WebSocket protocol**: POST `/callback/ws/endpoint` with `AppID`/`AppSecret` → get WSS URL → protobuf binary frames. Card action callbacks arrive as `event_type: "card.action.trigger"` with `frame_type: "event"` (not `"card"`).
- **Card callback ACK**: Response in ACK frame payload as `{"code": 200, "data": "<base64 of response JSON>"}`. Response JSON: `{"card": {"type": "raw", "data": {<card JSON>}}}`.
