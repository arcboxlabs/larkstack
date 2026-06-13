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
| `crates/lark-kit` | Shared toolkit for the Lark integration apps (sink, inbound server, config, crypto) |
| `apps/integrations/linear` | Linear webhook → Lark notifications + issue link previews (Integration) |
| `apps/integrations/github` | GitHub webhook → Lark notifications (Integration) |
| `apps/integrations/x` | X (Twitter) link previews in Lark (Integration) |
| `apps/automations/meeting-digest` | Auto-transcribe Lark VC recordings, post digest cards (Automation) |
| `apps/automations/standup-bot` | Daily standup reminders + on-demand actions (Automation) |

## Console features

- **Dashboard** — per-app state + last-error.
- **Live event stream** — every `tracing` event from every subsystem, SSE with `?since=` / `Last-Event-ID` backfill, persisted to SQLite (rolling 10k).
- **Config editor** — TOML editor in the UI; each app has an `enabled` toggle, and saving hot-restarts only the affected app.
- **Actions** — one-shot triggers per subsystem (`linear`/`github`: ping/test-lark, `x`: ping, `standup-bot: announce/ensure/remind/urgent/check`, `meeting-digest: process-meeting`).
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
# UI on http://localhost:8080; linear/github/x webhooks on :3000/:3001/:3002
```

Or with Docker:

```bash
docker compose up -d
```

See [docs/deploy/console.md](./docs/deploy/console.md) for the full env-var reference.

## Standalone apps

Each app keeps its own `[[bin]]` for local use or a single-purpose deploy, reading config from env vars (`LINEAR_*`, `LARK_*`, `GITHUB_*`, …):

```bash
cargo run -p linear      # or github / x / meeting-digest / standup-bot
```

For production, deploy the **console** — one binary, all apps, toggled from the UI. See [docs/deploy/console.md](./docs/deploy/console.md) and [docs/deploy/railway.md](./docs/deploy/railway.md).

## License

[MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE), at your option.
