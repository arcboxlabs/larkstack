# gitlab

GitLab project-webhook → Lark notification integration. The GitLab analog of the
`github` app: it receives GitLab webhooks on the console port and posts Lark group
cards (plus DMs for merge-request reviewers).

This file is `AGENTS.md`; `CLAUDE.md` is a symlink to it — edit `AGENTS.md`.

## Layout (flat, github-style — no DB, no scheduler, no API client)

- `app.rs` — `App`/`Instance` impl; `build()` parses config and requires at least one
  auth secret; `ingress_routes()` mounts the webhook router; `run()` publishes live
  `AppState` into the `lark_kit::StateSlot` via a `SlotGuard`.
- `config.rs` — `GitLabConfig` + `AppState`; `from_toml` / `from_env` overlay (env is the
  base, `[gitlab]` TOML wins), `lark_app` resolution against `[lark-apps]`.
- `routes.rs` — `ingress_router(slot)`: `POST /webhook` (mounted at `/webhooks/gitlab/`).
- `source/`
  - `payload.rs` — our own serde structs for the events we handle (the `gitlab` crate
    dropped its typed webhook structs, so we own the minimal subset) + a probe.
  - `verify.rs` — webhook auth (see below).
  - `handler.rs` — `webhook_handler`: authenticate → whitelist → dispatch on `object_kind`.
- `cards.rs` — event → Lark card builders.
- `actions.rs` — console actions: `ping`, `test-lark`.

## Flow

`POST /webhooks/gitlab/webhook` → `verify::authenticate` → project whitelist (on
`project.path_with_namespace`) → dispatch on the body's `object_kind`:

- `merge_request` — `open`/`reopen` → group card + DM mapped reviewers (fallback:
  assignees) via `user_map`; `merge` → merged card.
- `issue` — alert card only when a label in `alert_labels` is *newly added* (detected via
  the payload's `changes.labels` diff, so it fires once).
- `pipeline` — only `status == "failed"`.
- `note` — comment card (parent title resolved from `noteable_type`).
- `push` — branch-push card (skips zero-commit branch create/delete).

Cards link via URLs carried in the payload itself (`object_attributes.url`,
`project.web_url`), so the app works on gitlab.com and self-managed with no `base_url`.

## Auth

Two mechanisms, either accepted (signing preferred when present):

- **`X-Gitlab-Token`** — a *plaintext* shared secret (`[gitlab].webhook_token`), compared
  for equality. NOT an HMAC over the body. Works on every GitLab version.
- **Signing token** — GitLab 19.1+ Standard Webhooks (`[gitlab].signing_secret`, a
  `whsec_…` token). Verified via `lark_kit::verify_standard_webhook` (HMAC-SHA256 over
  `"{webhook-id}.{webhook-timestamp}.{body}"`, with a replay-window check).

## Config / env

`[gitlab]` section: `lark_app`, `webhook_token`, `signing_secret`, `user_map`
(GitLab username → Lark email), `alert_labels`, `project_whitelist` (matches
`path_with_namespace`); `[gitlab.lark]` for `webhook_url`/`base_url`. Env overrides:
`GITLAB_WEBHOOK_TOKEN`, `GITLAB_SIGNING_SECRET`, `GITLAB_USER_MAP` (JSON),
`GITLAB_ALERT_LABELS` (CSV), `GITLAB_PROJECT_WHITELIST` (CSV), plus `LARK_*`.

## Commands

```bash
cargo build -p gitlab
cargo test -p gitlab
cargo clippy -p gitlab --all-targets -- -D warnings
```
