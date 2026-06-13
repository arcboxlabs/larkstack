<div align="center">
  <strong>English</strong> | <a href="./README_zh.md">简体中文</a>
</div>

<br>

<h1 align="center">larkstack</h1>

<p align="center">
  Single-binary admin console + Lark/Feishu utility crates.
  <br>
  One process supervises everything, with a React Web UI for status, config, and one-shot actions.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-Edition_2024-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License">
</p>

<hr>

## What's in the box

| Crate | Purpose |
| :--- | :--- |
| `crates/larkstack-core` | Plug-in contract (`App`/`Instance`/`Manifest`) + control plane (`ControlPlane`, `EventStore`) |
| `crates/larkstack` | Framework host — per-app supervisor + axum API + embedded React UI |
| `crates/console` | Thin binary `larkstack-console` — registers the bundled apps and runs the host |
| `apps/integrations/linear-bridge` | Linear webhook → Lark notifications (Integration). Standalone bin still works |
| `apps/automations/meeting-digest` | Auto-transcribe Lark VC recordings, post digest cards (Automation) |
| `apps/automations/standup-bot` | Daily standup reminders + on-demand actions (Automation) |

## Console features

- **Dashboard** — per-app state + last-error.
- **Live event stream** — every `tracing` event from every subsystem, SSE with `?since=` / `Last-Event-ID` backfill, persisted to SQLite (rolling 10k).
- **Config editor** — TOML editor in the UI; each app has an `enabled` toggle, and saving hot-restarts only the affected app.
- **Actions** — one-shot triggers per subsystem (`linear-bridge: ping/test-lark`, `standup-bot: announce/ensure/remind/urgent/check`, `meeting-digest: process-meeting`).
- **Auth** — `CONSOLE_TOKEN` env var protects `/api/*`.

## Quick start

```bash
# 1. Build
cd crates/larkstack/web && npm ci && npm run build && cd ../../..
cargo build -p console --release

# 2. Run
CONSOLE_TOKEN=$(openssl rand -hex 32) \
LINEAR_WEBHOOK_SECRET=your_secret \
LARK_WEBHOOK_URL=https://open.larksuite.com/open-apis/bot/v2/hook/xxx \
./target/release/larkstack-console
# UI on http://localhost:8080, linear-bridge webhook on http://localhost:3000
```

Or with Docker:

```bash
docker compose up -d
```

See [docs/deploy/console.md](./docs/deploy/console.md) for the full env-var reference.

## Standalone subsystems

If you only need one piece, each crate keeps its `[[bin]]` and can be deployed alone:

| Target | Guide |
| :--- | :--- |
| linear-bridge → Railway/Docker | [docs/deploy/railway.md](./docs/deploy/railway.md) |

## License

[MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE), at your option.
