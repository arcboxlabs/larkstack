# Architecture

LarkStack is a three-layer pipeline. Sources receive webhooks and normalize them into
a unified `Event`, sinks format and deliver notifications, and the middle layer
(debounce + dispatch) operates only on `Event`. Adding a new source or sink requires
no changes to the other layers.

## Source code layout

```
apps/integrations/linear-bridge/src/
├── sources/
│   └── linear/          # Linear webhook source
│       ├── handler.rs   # POST /webhook — signature check, parse, normalize
│       ├── models.rs    # Linear JSON payload types
│       ├── client.rs    # GraphQL client (link previews)
│       └── utils.rs
├── sinks/
│   └── lark/            # Lark notification sink
│       ├── cards.rs     # Card builders (pure functions)
│       ├── bot.rs       # Tenant token cache + HTTP client
│       ├── webhook.rs   # Group webhook sender
│       ├── event_handler.rs  # POST /lark/event — challenge + link preview
│       └── models.rs
├── event.rs             # Unified Event enum (IssueCreated, IssueUpdated, CommentCreated)
├── dispatch.rs          # Routes an Event to all sinks
├── debounce.rs          # In-memory DebounceMap + tokio timers
├── config.rs            # AppState, env var parsing (figment)
├── main.rs              # Standalone entrypoint (axum + tokio)
└── lib.rs               # Library root (re-exports run + handle_actions)
```

## Data flow

A Linear webhook arrives at `POST /webhook`. The handler verifies the HMAC signature,
deserializes the payload, and converts it into an `Event`. The event goes through the
debounce layer (which coalesces rapid updates on the same issue within a configurable
window), then dispatch sends it to every registered sink.

Currently there is one source (Linear) and one sink (Lark), but the pipeline is
designed so either side can be extended independently.

## Dependencies

- `axum 0.8` — HTTP routing
- `tokio` — async runtime
- `reqwest 0.12` — HTTP client for outbound Lark / Linear API calls
- `figment` — config from env vars
- `larkoapi` — Lark Bot client + card types
- `hmac` + `sha2` — webhook signature verification
