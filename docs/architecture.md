# Architecture

larkstack is a **framework** that supervises pluggable **Apps** and ships them as one
admin-console binary. Apps come in two kinds — **Integrations** (external system → Lark)
and **Automations** (autonomous, in-Lark) — and plug into the host via the `App`/`Instance`
trait. The host owns the lifecycle: per-app enable toggle, build, run, status, and
crash/backoff restart.

## Workspace layout

```
crates/
  larkstack-core/   # App/Instance contract + control plane (status, events, config, Lark-app registry)
  larkstack/        # host lib: supervisor, axum admin API + SSE, SQLite store, config reload
  console/          # thin binary: registers the apps, runs the host
  lark-kit/         # shared toolkit for the Lark integration apps
apps/
  integrations/
    linear/         # Linear webhook → notifications + issue previews
    github/         # GitHub webhook → notifications
    x/              # X (Twitter) link previews
  automations/
    meeting-digest/ # Lark VC recordings → transcribe → digest cards
    standup-bot/    # daily standup scheduler + command bot
```

## Integrations + lark-kit

Each integration is its own crate that owns its **source** (webhook/callback parsing) and its
**cards**, and builds on `crates/lark-kit` for everything Lark-facing and source-agnostic:

- `card` — interactive-card builders (`card`, `message`, `md_div`, `link_button`)
- `webhook` / `bot` — group-webhook sender + DM bot (`larkoapi`)
- `server` — the inbound axum server (`serve(name, router, port, cancel)`)
- `config` — per-app `LarkConfig` / `ServerConfig` + `[lark-apps]` resolution
- `event` — Lark event-callback scaffold: `classify(body, token, encrypt_key) -> Callback`
  (AES-256-CBC decrypt, challenge handshake, token check, `url.preview.get`)
- `utils` — `verify_hmac_sha256`, `truncate`

There is **no** shared `Event` enum across apps — each builds cards directly from its own
source model and posts via `lark_kit::send_lark_card`. (`linear` keeps its debounce + an
`IssueNotification` model internally.)

## Data flow

- **Notifications** (linear, github): a webhook hits the app's server → the source verifies
  the HMAC, parses the payload, builds a card, and posts to the Lark group webhook (linear
  debounces rapid issue updates first; github DMs requested reviewers).
- **Link previews** (linear, x): Lark calls `POST /lark/event` → `lark_kit::event::classify`
  decrypts/validates and yields the URL → the app fetches details (Linear GraphQL / tweet
  API) and returns an inline preview card.

Each inbound app runs its own HTTP server, so the console config gives them distinct ports
(`[linear.server] 3000`, `[github.server] 3001`, `[x.server] 3002`).

## Lark credentials

Credentials live once under `[lark-apps.<name>]` (or are onboarded from the console's Lark
Apps tab, which live-tests them). An app binds to one with `lark_app = "<name>"`; the inline
`[<app>.lark]` section and `LARK_*` env vars still work for standalone runs.
