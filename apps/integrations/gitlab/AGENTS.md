# gitlab

GitLab project-webhook → Lark notification integration. Receives GitLab webhooks on the
console port and delivers Lark cards to **console-configured routing destinations** (group
chats / DMs), plus DMs to merge-request reviewers.

This file is `AGENTS.md`; `CLAUDE.md` is a symlink to it — edit `AGENTS.md`.

## Layout (flat, github-style — no DB, no scheduler, no API client)

- `app.rs` — `App`/`Instance`; `build()` parses config (requires an auth secret) and threads
  the `StateStore` into `AppState`; `routes()` mounts the routing admin router; `run()`
  publishes live `AppState` into the `lark_kit::StateSlot`.
- `config.rs` — `GitLabConfig` (secrets/bindings only) + `AppState` (`gitlab`, `bot`, `store`).
- `routes.rs` — `ingress_router(slot)`: `POST /webhook` (mounted at `/webhooks/gitlab/`).
- `source/{payload.rs, verify.rs, handler.rs}` — payload structs + probe, webhook auth, and
  `webhook_handler` (authenticate → dispatch on `object_kind` → deliver via routing).
- `cards.rs` — event → `LarkCard` builders (delivery is the caller's concern).
- `actions.rs` — console actions: `ping`, `test-notify`.

## Notification routing (console-configured, shared)

Routing lives in the shared `lark_kit::routing` module (reused by github). The handler loads
the config **fresh from the per-App `StateStore`** (`namespace = "gitlab"`) on every webhook,
so console edits apply with no restart. The config — rules (`{project/group → chat/DM}`),
default destinations, reviewer `user_map`, and `alert_labels` — is edited from the console's
**GitLab** tab (`GET/PUT /api/apps/gitlab/routing`). Delivery is bot-only (group chat by
`chat_id`, DM by user `open_id` or email), so a `lark_app` must be bound for notifications to send.

## Flow

`POST /webhooks/gitlab/webhook` → `verify::authenticate` → dispatch on the body's
`object_kind`; build the event's `LarkCard`; deliver to `cfg.destinations_for(subject, event)`
(subject = `project.path_with_namespace`):

- `merge_request` — `open`/`reopen` → card + DM mapped reviewers (fallback: assignees);
  `merge` → merged card.
- `issue` — alert card only when a label in `alert_labels` is *newly added* (`changes.labels`
  diff, so it fires once).
- `pipeline` — only `status == "failed"`. `note` — comment card. `push` — branch-push card
  (skips zero-commit branch create/delete).

Cards link via URLs in the payload itself, so the app works on gitlab.com and self-managed.

## Auth

Two mechanisms, either accepted (signing preferred when present):
- **`X-Gitlab-Token`** — a *plaintext* shared secret (`[gitlab].webhook_token`), compared for
  equality. NOT an HMAC.
- **Signing token** — GitLab 19.1+ Standard Webhooks (`[gitlab].signing_secret`, a `whsec_…`
  token), verified via `lark_kit::verify_standard_webhook`.

## Config / env

`config.toml` carries only secrets/bindings; routing is in the console (StateStore).
`[gitlab]`: `enabled`, `lark_app` (binds a `[lark-apps]` for the bot), `webhook_token`,
`signing_secret`. Env overrides: `GITLAB_WEBHOOK_TOKEN`, `GITLAB_SIGNING_SECRET`, plus
`LARK_*` for the bot when not using `lark_app`.

## Commands

```bash
cargo build -p gitlab
cargo test -p gitlab
cargo clippy -p gitlab --all-targets -- -D warnings
```
