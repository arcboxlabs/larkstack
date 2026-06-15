# Lark Stack - Integration Hub for Lark

**An open-source integration hub for Lark / Feishu** — a Slack-grade ecosystem in one self-hosted binary.

One process supervises every integration and automation, with a React console for status, config, and one-shot actions.

![Rust](https://img.shields.io/badge/Rust-Edition_2024-orange.svg)
![License](https://img.shields.io/badge/License-MIT-blue.svg)

[![README in English](https://img.shields.io/badge/English-d9d9d9)](./README.md)
[![简体中文 README](https://img.shields.io/badge/简体中文-d9d9d9)](./README_CN.md)

<hr>

## Why Lark Stack

Slack won startups on its ecosystem, not its chat. Thousands of integrations mean every tool a team already lives in shows up where they talk — a PR, a deploy, a paid invoice, an on-call page, a CRM update.

But Lark's integration catalog is thin, especially outside China — so choosing it too often means giving up that ecosystem.

**Lark Stack closes that gap.** A self-hosted, open-source hub that packs the missing Slack-grade integrations into one binary you toggle from a web console. And it's not only bridges: it also runs autonomous in-Lark automations (standup, meeting minutes, …) — growing the ecosystem deeper, not just wider. The `App`/`Instance` contract plus `lark-kit` make every new app a small, self-contained crate.

The goal: a startup or business can pick Lark and lose nothing — every external tool bridged in, every recurring workflow run by a built-in automation, all where their team already works.

## Apps

Apps are pluggable units the console supervises and toggles. **Integrations** bridge an external system into Lark; **Automations** run autonomously on a schedule or event. Click a name for its docs.

| App | Kind | What it does |
| :--- | :--- | :--- |
| [`Linear`](./apps/integrations/linear) | ![Integration][kind-integration] | Linear webhook → Lark notification cards + issue link previews |
| [`GitHub`](./apps/integrations/github) | ![Integration][kind-integration] | GitHub webhook → PR/issue/CI/security-alert cards + review-request DMs |
| [`X`](./apps/integrations/x) | ![Integration][kind-integration] | X (Twitter) link previews rendered as Lark cards (preview-only) |
| [`Minutes`](./apps/automations/minutes) | ![Automation][kind-automation] | Auto-transcribe Lark VC recordings (STT) → digest cards + optional Lark Doc |
| [`Standup`](./apps/automations/standup) | ![Automation][kind-automation] | Daily standup reminders + on-demand commands (announce/remind/urgent/check) |

[kind-integration]: https://img.shields.io/badge/Integration-2563eb?style=flat-square
[kind-automation]: https://img.shields.io/badge/Automation-16a34a?style=flat-square

The three integrations are served on the console port under `/webhooks/<app>/` (e.g. `/webhooks/linear/webhook`, `/webhooks/github/webhook`, `/webhooks/x/lark/event`) — no per-app ports.

## Roadmap

The bridges above are a starting set; the ambition is category coverage that matches what a team expects from Slack's App Directory. Want one sooner — or one that's not listed? [Open an issue](../../issues) or send a PR: a new integration is one self-contained crate.

| Category | Shipped | Next up |
| :--- | :--- | :--- |
| Dev & code | Linear, GitHub | GitLab, Jira, Sentry |
| CI/CD & deploy | GitHub CI | Vercel, Netlify, generic webhook |
| Incident & on-call | — | PagerDuty, Opsgenie, incident.io |
| Observability | — | Datadog, Grafana, Alertmanager |
| Revenue & growth | — | Stripe, HubSpot |
| Support | — | Zendesk, Intercom |
| Social & feeds | X / Twitter | RSS, status pages |
| Team rituals | Standup | Polls, calendar digests, retros |

## Framework

| Crate | Purpose |
| :--- | :--- |
| `crates/larkstack-core` | Plug-in contract (`App`/`Instance`/`Manifest`) + control plane (`ControlPlane`, `EventStore`) |
| `crates/larkstack` | Framework host — per-app supervisor + axum API + embedded React UI |
| `crates/console` | Thin binary `larkstack-console` — registers the bundled apps and runs the host |
| `crates/lark-kit` | Shared toolkit for the Lark integration apps (sink, inbound server, config, crypto) |

## Console features

- **Dashboard** — per-app state + last-error.
- **Live event stream** — every `tracing` event from every subsystem, SSE with `?since=` / `Last-Event-ID` backfill, persisted to SQLite (rolling 10k).
- **Config editor** — TOML editor in the UI; each app has an `enabled` toggle, and saving hot-restarts only the affected app.
- **Actions** — one-shot triggers per subsystem (`linear`/`github`: ping/test-lark, `x`: ping, `standup: announce/ensure/remind/urgent/check`, `minutes: process-meeting`).
- **Auth** — sign in with **Lark OAuth**; `/api/*` needs a session. The console
  is open until you bind a Lark app under `[console]`, then restricted to the
  `admins` allowlist.

## Quick start

```bash
# 1. Build
cd dashboard && pnpm install --frozen-lockfile && pnpm build && cd ..
cargo build -p console --release

# 2. Run (console is open until you set up Lark OAuth from the UI)
CONSOLE_SECRET=$(openssl rand -hex 32) \
LINEAR_WEBHOOK_SECRET=your_secret \
LARK_WEBHOOK_URL=https://open.larksuite.com/open-apis/bot/v2/hook/xxx \
./target/release/larkstack-console
# UI + API + webhooks all on http://localhost:8080 (webhooks at /webhooks/<app>/)
# CONSOLE_SECRET (optional) keeps sessions valid across restarts; a key is
# auto-generated and persisted if unset.
```

Or with Docker:

```bash
docker compose up -d
```

See [docs/deploy/console.md](./docs/deploy/console.md) for the full env-var reference.

## Standalone apps

The automation apps keep their own `[[bin]]` for local/CLI use, reading config from env vars (`LARK_*`, `STANDUP_*`, …). The integrations (linear/github/x) are libraries — run them through the console.

```bash
cargo run -p standup     # or minutes
```

For production, deploy the **console** — one binary, all apps, toggled from the UI. See [docs/deploy/console.md](./docs/deploy/console.md) and [docs/deploy/railway.md](./docs/deploy/railway.md).

## License

[MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE), at your option.
