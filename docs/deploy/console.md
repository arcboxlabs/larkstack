# Deploy `larkstack-console`

The console is a single binary that supervises the bundled apps (`linear`,
`github`, `x`, `minutes`, `standup`) and serves a React admin UI.
One process, one deploy. The admin API is self-documented — OpenAPI spec at
`/api/openapi.json`, a Scalar explorer at `/api/docs`.

## Environment

| Variable | Default | Purpose |
| :--- | :--- | :--- |
| `CONSOLE_PORT` | `8080` | Listener for the admin UI + `/api/*` |
| `CONSOLE_SECRET` | _unset_ | Derives the session-cookie signing key, so logins survive restarts. **Unset = a random key is generated and persisted** to `session.key` in the data dir. |
| `CONSOLE_DATA_DIR` | `./data` | Directory for `events.db` (event log, 10k rolling), `config.toml`, `state.db`/`metrics.db`, and `session.key`. Mount as a volume. |
| `RUST_LOG` | `info` | tracing filter; same syntax as `env_logger` |

## Authentication

Sign-in is **Lark OAuth**. Until a Lark app is bound the console is **open** (so
you can reach the UI to set it up); a warning is logged and a banner is shown.

**From the UI (recommended).** First boot lands on a guided **Setup** screen:
register a Lark app (credentials are live-tested), then bind it as the console's
sign-in client and set the admin allowlist — no TOML editing. The screen shows
the exact redirect URI to register in the Lark app's security settings (grant it
the user-info permission). Saving enforces sign-in immediately and hands you to
the login flow.

**Or in `config.toml`,** the equivalent binding under `[console]`:

```toml
[console]
lark_app = "main"               # one of the [lark-apps] entries
admins = ["you@example.com"]    # email allowlist; empty = any tenant user
# redirect_uri = "https://console.example.com/auth/callback"  # else auto-derived
```

Register `<console-url>/auth/callback` as a redirect URI in the Lark app's
security settings, and grant it the user-info permission. If you ever lock
yourself out (admins list omits your account), clear `[console].lark_app` in
`config.toml` on the server to reopen the console.

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
  -e CONSOLE_SECRET=$(openssl rand -hex 32) \
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

- Sign-in is a signed session **cookie** (`lk_session`), set on `/auth/callback`
  and sent with every request (SSE included, since `EventSource` is same-origin).
  Make sure your proxy forwards cookies and preserves `Set-Cookie`.
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
cd dashboard && pnpm install --frozen-lockfile && pnpm build && cd -
cargo build -p console --release
# binary at target/release/larkstack-console
CONSOLE_SECRET=$(openssl rand -hex 32) CONSOLE_DATA_DIR=./data \
  ./target/release/larkstack-console
```

`crates/larkstack/build.rs` writes a placeholder `dashboard/dist/index.html` if
the frontend hasn't been built, so `cargo build -p console` always succeeds —
the UI just shows a "build the frontend" hint until you run `npm run build`.

## Logs

stdout (via `tracing-subscriber`'s fmt layer) + an in-process broadcast for
the SSE stream that the UI consumes. Container platforms collecting stdout
will see the same lines that show up in the events tab.

## Standalone subsystems

Each app still has its own `[[bin]]` for standalone deployments
(`cargo run -p linear`, `-p github`, `-p x`, …). Use those when you only need
one piece. See [Deploy to Railway / Docker](railway.md).
