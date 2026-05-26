# Deploy `larkstack-console`

The console is a single binary that supervises `linear-bridge`,
`meeting-digest`, and `standup-bot` and serves a React admin UI. One process,
one deploy.

## Environment

| Variable | Default | Purpose |
| :--- | :--- | :--- |
| `CONSOLE_PORT` | `8080` | Listener for the admin UI + `/api/*` |
| `CONSOLE_TOKEN` | _unset_ | Required value of `Authorization: Bearer <token>` for `/api/*` (except `/api/health`). **Unset = no auth** — the console logs a warning and keeps running. Only acceptable on a private network. |
| `CONSOLE_DATA_DIR` | `./data` | Directory for `events.db` (SQLite event log, 10k rolling) and `config.toml`. Mount as a volume. |
| `RUST_LOG` | `info` | tracing filter; same syntax as `env_logger` |

Each subsystem's own env vars (`LINEAR_*`, `LARK_*`, `STT_*`, `DIGEST_*`,
`STANDUP_*`, `PORT`, `DEBOUNCE_DELAY_MS`) are read by its config loader as
fallback defaults. Anything also set in `config.toml` overrides the env at
runtime; anything left empty in the TOML keeps the env value. Secrets are
usually kept in env vars; ops fields are edited from the UI.

## Docker

```bash
docker build -t larkstack-console .
docker run -d --name larkstack-console \
  -p 8080:8080 -p 3000:3000 \
  -e CONSOLE_TOKEN=$(openssl rand -hex 32) \
  -e LINEAR_WEBHOOK_SECRET=... \
  -e LARK_APP_ID=... -e LARK_APP_SECRET=... \
  -v larkstack-data:/data \
  larkstack-console
```

The image bundles the React build via `rust-embed`. First boot writes a
default `/data/config.toml` you can edit in the UI.

The bundled `docker-compose.yml` covers the same setup with a named volume.

## Reverse proxy

If you put the console behind nginx/Caddy/Cloudflare:

- The UI uses `Authorization: Bearer …` for HTTP and `?token=…` for SSE
  (because `EventSource` doesn't support custom headers). Make sure your
  proxy doesn't strip either.
- Disable response buffering for `/api/events` so SSE events arrive in real
  time (nginx: `proxy_buffering off;`, Caddy: `flush_interval -1`).
- Forward `Last-Event-ID` so SSE clients can backfill after a reconnect.

## Health checks

`GET /api/health` returns `"ok"` and is exempt from auth — wire it into your
orchestrator's liveness / readiness probe. The Dockerfile includes a
`HEALTHCHECK` that hits this endpoint every 30s.

## Graceful shutdown

The console handles `SIGINT` and `SIGTERM` by closing the listener and
draining in-flight HTTP requests before exiting. Subsystems get aborted
during shutdown.

## Bundling without Docker

```bash
cd crates/console/web && npm ci && npm run build && cd -
cargo build -p console --release
# binary at target/release/larkstack-console
CONSOLE_TOKEN=$(openssl rand -hex 32) CONSOLE_DATA_DIR=./data \
  ./target/release/larkstack-console
```

`crates/console/build.rs` writes a placeholder `web/dist/index.html` if the
frontend hasn't been built, so `cargo build -p console` always succeeds —
the UI just shows a "build the frontend" hint until you run `npm run build`.

## Logs

stdout (via `tracing-subscriber`'s fmt layer) + an in-process broadcast for
the SSE stream that the UI consumes. Container platforms collecting stdout
will see the same lines that show up in the events tab.

## Standalone subsystems

Each subsystem still has its own `[[bin]]` for standalone deployments
(`cargo run -p linear-bridge`, etc.). Use those when you only need one
piece — for example, the existing CF Worker deploy of `linear-bridge`. See
the per-crate docs:

- [linear-bridge → Cloudflare Workers](cloudflare-workers.md)
- [linear-bridge → Railway](railway.md)
