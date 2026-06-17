# github

GitHub webhook → Lark notification integration. Receives GitHub webhooks on the console
port and delivers Lark cards to **console-configured routing destinations** (group chats /
DMs), plus DMs to PR reviewers.

This file is `AGENTS.md`; `CLAUDE.md` is a symlink to it — edit `AGENTS.md`.

## Layout (flat — no DB, no scheduler)

- `app.rs` — `App`/`Instance`; `build()` requires `webhook_secret` and threads the
  `StateStore` into `AppState`; `routes()` mounts the routing admin router; `run()` publishes
  live `AppState` into the `lark_kit::StateSlot`.
- `config.rs` — `GitHubConfig` (secrets only) + `AppState` (`github`, `bot`, `store`).
- `routes.rs` — `ingress_router(slot)`: `POST /webhook` (mounted at `/webhooks/github/`).
- `source/{handler.rs, utils.rs}` — `webhook_handler` (verify HMAC → octocrab parse →
  dispatch → deliver via routing) + the `X-Hub-Signature-256` verifier.
- `cards.rs` — event → `LarkCard` builders. `actions.rs` — `ping`, `test-notify`.

## Notification routing (console-configured, shared)

Routing lives in the shared `lark_kit::routing` module (reused by gitlab). The handler loads
the config **fresh from the per-App `StateStore`** (`namespace = "github"`) on every webhook,
so console edits apply with no restart. The config — rules (`{repo/org → chat/DM}`), default
destinations, reviewer `user_map`, and `alert_labels` — is edited from the console's
**GitHub** tab (`GET/PUT /api/apps/github/routing`). Delivery is bot-only (group chat by
`chat_id`, DM by user `open_id` or email), so a `lark_app` must be bound for notifications to send.

## Flow

`POST /webhooks/github/webhook` (HMAC `X-Hub-Signature-256`) → octocrab `WebhookEvent` →
build a `LarkCard` → deliver to `cfg.destinations_for(repo, event)` (subject = repo
`full_name`):

- `pull_request` — opened / review-requested (+ reviewer DM via `user_map`) / merged.
- `issues` — alert card when a label in `alert_labels` is applied.
- `workflow_run` — failed CI. `secret_scanning` / `dependabot` — security alerts
  (critical/high). Card links come from the payload.

## Config / env

`config.toml` carries only secrets/bindings; routing is in the console (StateStore).
`[github]`: `enabled`, `lark_app` (binds a `[lark-apps]` for the bot), `webhook_secret`
(required; HMAC for `X-Hub-Signature-256`). Env overrides: `GITHUB_WEBHOOK_SECRET`, plus
`LARK_*` for the bot when not using `lark_app`.

## Commands

```bash
cargo build -p github
cargo test -p github
cargo clippy -p github --all-targets -- -D warnings
```
