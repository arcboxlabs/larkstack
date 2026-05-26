<div align="center">
  <a href="./README.md">English</a> | <strong>简体中文</strong>
</div>

<br>

<h1 align="center">larkstack</h1>

<p align="center">
  一个可执行文件管理多个 Lark/Feishu 工具，自带 React 管理 UI（状态、配置、动作触发）。
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-Edition_2024-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License">
</p>

<hr>

## 仓库结构

| Crate | 作用 |
| :--- | :--- |
| `crates/console` | umbrella binary `larkstack-console`——tokio 监管 + axum API + 内嵌 React UI |
| `crates/control` | 共享类型（`ControlPlane`、`EventStore`、动作分发） |
| `crates/linear-bridge` | Linear webhook → 飞书通知。仍可独立部署（含 Cloudflare Workers） |
| `crates/meeting-digest` | 自动转写飞书 VC 录制并发送摘要卡片 |
| `crates/standup-bot` | 每日站会提醒 + 群内命令 |

## 控制台特性

- **状态面板**——每个子系统的运行状态 + 最近错误
- **实时事件流**——所有子系统的 `tracing` 事件，SSE 支持 `?since=` / `Last-Event-ID` 回放，持久化到 SQLite（滚动 1 万条）
- **配置编辑**——UI 内 TOML 编辑器，保存即热重启对应子系统
- **动作触发**——每个子系统的一次性命令（`linear-bridge: ping/test-lark`、`standup-bot: announce/ensure/remind/urgent/check`、`meeting-digest: process-meeting`）
- **认证**——`CONSOLE_TOKEN` 环境变量保护 `/api/*`

## 快速开始

```bash
# 1. 构建
cd crates/console/web && npm ci && npm run build && cd ../../..
cargo build -p console --release

# 2. 运行
CONSOLE_TOKEN=$(openssl rand -hex 32) \
LINEAR_WEBHOOK_SECRET=your_secret \
LARK_WEBHOOK_URL=https://open.larksuite.com/open-apis/bot/v2/hook/xxx \
./target/release/larkstack-console
# UI 在 http://localhost:8080，linear-bridge webhook 在 http://localhost:3000
```

或者用 Docker：

```bash
docker compose up -d
```

完整环境变量见 [docs/deploy/console.md](./docs/deploy/console.md)。

## 独立部署单个子系统

每个 crate 仍保留独立 `[[bin]]`：

| 目标 | 文档 |
| :--- | :--- |
| linear-bridge → Railway/Docker | [docs/deploy/railway.md](./docs/deploy/railway.md) |
| linear-bridge → Cloudflare Workers | [docs/deploy/cloudflare-workers.md](./docs/deploy/cloudflare-workers.md) |

## 许可证

[MIT](./LICENSE-MIT) 或 [Apache-2.0](./LICENSE-APACHE)，二选一。
