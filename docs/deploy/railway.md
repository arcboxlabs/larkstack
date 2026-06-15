# Deploy the console to Railway / Docker

The deploy artifact is the **console** — one binary that supervises every app,
built from the workspace-root `Dockerfile`.

## Railway

1. Create a new project on [Railway](https://railway.app/) and connect this repository.
2. Leave **Root Directory** at the repo root so the workspace `Dockerfile` resolves.
3. Add environment variables — `CONSOLE_SECRET` (a random value; keeps logins
   valid across restarts) plus any app secrets; see
   [Configuration](../getting-started/configuration.md). The `[console]`
   Lark-OAuth binding and app credentials can also be set from the Config /
   Lark Apps tabs. Register `<your-public-url>/auth/callback` as a redirect URI
   in the Lark app.
4. Railway detects the `Dockerfile` and builds on push. The admin UI, API, and every
   integration's webhooks serve on `$CONSOLE_PORT` (default `8080`).
5. Enable the apps you want from the UI, then point Linear/GitHub/Lark at the public
   webhook URL under `/webhooks/<app>/` — `/webhooks/linear/webhook`,
   `/webhooks/github/webhook`, `/webhooks/x/lark/event`.

## Manual Docker build

```bash
docker build -t larkstack-console .
docker run -p 8080:8080 \
  -e CONSOLE_SECRET=$(openssl rand -hex 32) \
  -v larkstack-data:/data \
  larkstack-console
```

`docker compose up -d` does the same via [`docker-compose.yml`](../../docker-compose.yml).

> The integration apps (linear/github/x) are libraries with no standalone binary — they
> run inside the console. Only the automations (`minutes`/`standup`) keep a `[[bin]]`.
