# AGENTS.md — linear

Linear integration: Linear webhook → Lark notifications, plus Linear issue link
previews inside Lark. Builds on `lark-kit` (Lark sink/config/crypto + the ingress state slot) and
`larkstack-core` (the `App`/`Instance` contract). See the repo-root `AGENTS.md`
for framework-level context; this file covers only what's specific to this crate.

## Layout (organized by boundary)

The tree follows the data path — HTTP in, normalize, adapt out:

- **`routes/`** — inbound HTTP surface (mounted by the host at `/webhooks/linear/`).
  - `mod.rs` — `ingress_router(slot)`: builds the `Router` whose handlers read live
    `AppState` from the `lark_kit::StateSlot` via the `Live` extractor.
  - `webhook.rs` — `POST /webhooks/linear/webhook`: HMAC-verify → parse → normalize → debounce/dispatch.
  - `preview.rs` — `POST /webhooks/linear/lark/event`: Lark `url.preview.get` callback → link preview.
- **`domain/`** — normalized core, independent of HTTP/Linear/Lark.
  - `mod.rs` — `IssueNotification`, `Priority` (Linear's 0–4 scale normalized), and
    `team_key` (the routing subject — an identifier's prefix, `ENG-42` → `ENG`).
  - `debounce.rs` — `DebounceMap`: coalesces rapid issue updates into one card.
- **`source/`** — the Linear adapter (the external system bridged *from*).
  - `payload.rs` — deserialization types for inbound **webhook** JSON.
  - `changes.rs` — diffs an updated issue vs its previous state → change lines.
  - `api.rs` — `LinearClient` GraphQL client + the `graphql_client`-generated
    bindings + hand-mapped projections (`LinearIssueData`, `DueIssue`,
    `IssueSubscriberInfo`, `LinearUser`) + `extract_identifier_from_url`.
- **`lark/`** — the Lark adapter (bridged *to*).
  - `cards.rs` — card builders: group-chat (`issue_card`/`comment_card`) and DM
    (`assign_dm`/`reminder_dm`/`subscriber_issue_dm`/`subscriber_comment_dm`/`preview_card`).
    All return `LarkCard` — group cards now ship through the bot (routing), not a webhook.
  - `notify.rs` — DM senders only (single `dm`, `dm_many` fan-out); group cards go
    through `lark_kit::routing::deliver_all` from `routes::webhook`.
- **`scheduler.rs`** — background due-date reminder loop (runs alongside the
  webhook server, both honoring `cancel`). Polls `fetch_issues_due_soon`, picks
  the applicable cadence tier, dedupes via `db::due_reminders`, DMs the
  recipients. No-op while reminders are disabled or `LINEAR_API_KEY` is unset.
- **`db/`** — the app's persistence layer (mirrors `larkstack_core::db`), one
  submodule per concern; every table is namespaced `linear_`. `db::migrations()`
  aggregates all of them for `App::migrations`.
  - `user_map/` — admin Linear-email → Lark-email overrides (`linear_user_map`);
    `resolve_lark_email`; admin CRUD at `/api/apps/linear/user-map`.
  - `settings/` — the single-row `linear_settings`: admin-tunable subscriber
    fan-out scope + reminder cadence/recipients. `Settings::load` (row or code
    defaults), `save`; admin `GET/PUT /api/apps/linear/settings`. Read live by the
    scheduler and webhook — changes apply without a restart.
  - `due_reminders/` — dedup ledger (`linear_due_reminders`, PK
    `(issue_id, due_date, tier)`); `already_sent`/`record` so each cadence tier
    fires once per deadline.
- **top level** — wiring: `app.rs` (`App`/`Instance`; `ingress_routes` publishes the
  `StateSlot`, `run` publishes live state + the bot (for the routing `/chats` route) +
  drives the reminder scheduler, `migrations`/`routes`), `config.rs` (`AppState` +
  TOML/env; holds both the `db` and the `StateStore`), `actions.rs` (console actions:
  `ping`, `test-notify` — sends a test card to a `chat`/`dm` target), `lib.rs`.

## Notification routing (console-configured, shared)

Group-chat delivery uses the shared `lark_kit::routing` module (same as github/gitlab).
The webhook loads the config **fresh from the per-App `StateStore`** (`namespace =
"linear"`) on every event, so console edits apply with no restart. The config — rules
(`{team-key → chat/DM}`) + default destinations — is edited from the console's **Linear**
tab (`GET/PUT /api/apps/linear/routing`, plus `GET /api/apps/linear/chats` for the
chat-picker). The **routing subject is the team key** (`domain::team_key`, the identifier
prefix), and the **event** is `"issue"` (create/update) or `"comment"`. So "all updates →
one chat" is a `match = "*"` rule (or a default destination); "team ENG → chat X" is `match
= "ENG"`. Delivery is **bot-only** (group chat by `chat_id`, DM by email), so a `lark_app`
must be bound for group notifications to send.

Routing's own `user_map`/`alert_labels` fields are **unused** here: Linear keeps its
richer DB-backed `user_map` (Linear-email → Lark-email, below) for assignee/subscriber
DMs, and has no label-triggered alerts. The console's Linear routing editor hides both.

## Flows

**Notifications** (`POST /webhooks/linear/webhook`): verify `linear-signature` HMAC → parse
`source::payload::LinearPayload` → match `(type, action)`:
- `Issue` create/update → `domain::IssueNotification` → `domain::debounce` (merge
  within the window) → `lark::cards::issue_card` → `routing::deliver_all` to
  `cfg.destinations_for(team_key, "issue")` + assignee `notify::dm`. The assignee DM
  target is the Linear email resolved through `db::user_map::resolve_lark_email`
  (override if any, else unchanged).
- `Comment` create → `lark::cards::comment_card` → `routing::deliver_all` to
  `cfg.destinations_for(team_key, "comment")` (no debounce).

**Subscriber fan-out** (per-person DMs, layered on the above): when an event
qualifies under `settings` — comments (`subscriber_on_comment`), status changes
(`subscriber_on_status_change`, detected via `updatedFrom.state`), or any update
(`subscriber_on_any_update`) — `webhook::notify_subscribers` calls
`LinearClient::fetch_issue_subscribers`, drops inactive users, no-email users, and
the triggering actor (by id, from the webhook `actor` / comment `data.user`),
resolves each via `user_map`, and `notify::dm_many`. Needs `LINEAR_API_KEY`
(subscriber emails come from GraphQL). The assignment DM is separate and unchanged.

**Due-date reminders** (`scheduler.rs`, background): each tick loads `settings`,
computes today-in-tz, fetches `fetch_issues_due_soon` over `[today − overdue_max,
today + max(lead_days)]`, and for each issue picks `domain::reminders::current_tier`
(smallest lead ≥ days-until; negative per-day tiers once overdue). Unsent tiers
(`db::due_reminders`) DM the assignee — and subscribers when
`reminder_recipients = assignee_and_subscribers` — then record. Cadence/recipients
are admin-tuned in `settings`; default `T-7/T-3/T-1/day-of` + daily overdue (cap 7).
Needs `LINEAR_API_KEY`.

**Link previews** (`POST /webhooks/linear/lark/event`): `lark_kit::event::classify`
(decrypt/token/challenge) → `source::api::extract_identifier_from_url` →
`LinearClient::fetch_issue_by_identifier` (GraphQL) → `lark::cards::preview_card`
→ inline reply. Requires `LINEAR_API_KEY`; no-ops without it.

## GraphQL — the `source::api` side

- Bindings are generated by **`graphql_client`** from the `graphql/*.graphql`
  operations (`issue_by_number`, `issues_due_soon`, `issue_subscribers`) against
  the committed **`graphql/schema.graphql`**, validated at compile time. Paths in
  the `#[derive(GraphQLQuery)]` attr are relative to the crate root
  (`CARGO_MANIFEST_DIR`), so they're unaffected by where `api.rs` sits.
- Custom scalars are mapped by module-level type aliases in `api.rs`
  (`type TimelessDate = String; type TimelessDateOrDuration = String;`) — that's
  how `graphql_client` resolves them. All ops share one generic `run::<Q>` helper.
- `graphql/schema.graphql` is a **pinned lock** (Linear's published SDL, ~1.2 MB).
  Builds read it offline. Refresh it deliberately with the **`update-linear-schema`**
  devenv script (pulls Linear's SDK SDL), then commit — never hand-edit.
- Responses are hand-mapped to small projections (`LinearIssueData`, `DueIssue`,
  `IssueSubscriberInfo`, `LinearUser`) so `lark::cards`/`scheduler` never depend on
  generated types.

## Webhook payloads are hand-written on purpose — the `source::payload` side

`payload.rs` types are a **deliberate minimal subset** of Linear's webhook JSON,
NOT generated. Linear *does* model webhooks in the SDL (`EntityWebhookPayload`,
`IssueWebhookPayload`, …), but:
- `graphql_client` is query-driven and those types are unreachable from any query,
  so it cannot emit them.
- The envelope's `data` is a union with no `__typename` on the wire (the variant
  comes from the top-level `type` field), so generic codegen can't deserialize it
  anyway — `webhook.rs` dispatches on `(type, action)` by hand.
- `updatedFrom` is `JSONObject` (untyped) even in the SDL.

Treat the SDL's `*WebhookPayload` types as a **reference** to check `payload.rs`
for drift, not as a generator.

## Config / env

`App::build` reads `[linear]` from the full config TOML, overlaying env per field.
`config.toml` carries only secrets/bindings; **behavioral toggles live in the
`linear_settings` DB row and the routing config in the `StateStore`**, both edited live
from the console's **Linear** tab (no restart).
- `LINEAR_WEBHOOK_SECRET` — HMAC secret (required for the webhook path).
- `LINEAR_API_KEY` — Linear GraphQL key; **optional**, but required for link
  previews, **due-date reminders, and subscriber fan-out** (all no-op without it).
- `LARK_*` / `lark_app = "<name>"` — Lark bot, used for **both** group-chat and DM
  delivery (routing is bot-only); see root `AGENTS.md`.
- `[linear].debounce_delay_ms` (default `5000`) — issue-update coalescing window.

## Commands

```bash
cargo build -p linear
cargo clippy -p linear --all-targets -- -D warnings
cargo test -p linear
update-linear-schema           # refresh graphql/schema.graphql (in the devenv shell)
```
