# Deploy the console to Railway / Docker

The deploy artifact is the **console** — one binary that supervises every app,
built from the workspace-root `Dockerfile`.

## Railway

1. Create a new project on [Railway](https://railway.app/) and connect this repository.
2. Leave **Root Directory** at the repo root so the workspace `Dockerfile` resolves.
3. Add environment variables — at minimum `CONSOLE_TOKEN` (protects `/api/*`); see
   [Configuration](../getting-started/configuration.md) for the full list. App
   credentials can also be set from the console's Config / Lark Apps tabs.
4. Railway detects the `Dockerfile` and builds on push. The admin UI + API serve on `$CONSOLE_PORT` (default `8080`).
5. Enable the apps you want from the UI; each inbound integration serves its webhook on its
   own port (`[linear.server] 3000`, `[github.server] 3001`, `[x.server] 3002`) — expose the
   ones you need and point Linear/GitHub/Lark at the matching public URL
   (`/webhook`, `/github/webhook`, `/lark/event`).

## Manual Docker build

```bash
docker build -t larkstack-console .
docker run -p 8080:8080 -p 3000:3000 \
  -e CONSOLE_TOKEN=$(openssl rand -hex 32) \
  -v larkstack-data:/data \
  larkstack-console
```

`docker compose up -d` does the same via [`docker-compose.yml`](../../docker-compose.yml).

> Need just one integration? Each app keeps a `[[bin]]` (`cargo run -p github`) and
> reads `GITHUB_*` / `LARK_*` from the environment — package it yourself if you want a
> single-purpose image.
