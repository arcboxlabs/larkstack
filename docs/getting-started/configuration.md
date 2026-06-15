# Configuration

Set these environment variables before running. On Railway / Docker, add them in the
platform dashboard.
![Linear API Configuration](../images/linear-api-config.jpeg)

## Environment variables

| Variable | Required | Description |
| :--- | :---: | :--- |
| `LINEAR_WEBHOOK_SECRET` | Yes | HMAC-SHA256 signature verification for Linear webhooks |
| `LARK_WEBHOOK_URL` | Yes | Lark group chat webhook URL |
| `LARK_APP_ID` | No | Bot app ID — enables DM on assign |
| `LARK_APP_SECRET` | No | Bot app secret — pair with `LARK_APP_ID` |
| `LINEAR_API_KEY` | No | GraphQL API access — enables link previews |
| `LARK_VERIFICATION_TOKEN` | No | Lark event callback verification |

The issue-update coalescing window is no longer an env var — set
`[linear].debounce_delay_ms` (default `5000`) in `config.toml` instead. The
console serves the integration on one port; there is no per-app listen port.

## Feature tiers

The two required variables give you group notifications. Optional variables unlock
additional features:

1. **Base** (`LINEAR_WEBHOOK_SECRET` + `LARK_WEBHOOK_URL`) — group chat cards on
   issue create, update, and comment.
2. **DM on assign** (add `LARK_APP_ID` + `LARK_APP_SECRET`) — private message to
   the assignee when an issue is assigned.
3. **Link previews** (add `LINEAR_API_KEY` + `LARK_VERIFICATION_TOKEN`) — paste a
   `linear.app` URL in Lark and it unfurls into a summary card.
