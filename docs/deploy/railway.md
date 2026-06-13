# Deploy to Railway / Docker

The `Dockerfile` lives in `apps/integrations/linear-bridge/` for [Railway](https://railway.app/).

## Steps

1. Create a new project on Railway and connect this repository.
2. In the Railway service settings, set **Root Directory** to `apps/integrations/linear-bridge`
   so the Dockerfile and build context resolve correctly.
3. Add environment variables in the Railway dashboard — see
   [Configuration](../getting-started/configuration.md) for the full list.

![Railway Variables](../images/railway-vars.png)

4. Railway detects the `Dockerfile` and builds on push.
5. Set the Linear webhook URL to `https://<your-app>.up.railway.app/webhook`.
6. Set the Lark event callback URL to `https://<your-app>.up.railway.app/lark/event`.

## Manual Docker build

```bash
docker build -t linear-bridge apps/integrations/linear-bridge
docker run -p 3000:3000 \
  -e LINEAR_WEBHOOK_SECRET=your_secret \
  -e LARK_WEBHOOK_URL=https://open.larksuite.com/open-apis/bot/v2/hook/xxx \
  linear-bridge
```
