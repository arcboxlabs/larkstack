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
| `crates/larkstack-core` | 插件契约（`App`/`Instance`/`Manifest`）+ 控制面（`ControlPlane`、`EventStore`） |
| `crates/larkstack` | 框架宿主——按 app 监管 + axum API + 内嵌 React UI |
| `crates/console` | 瘦二进制 `larkstack-console`——注册内置 apps 并运行宿主 |
| `crates/lark-kit` | 各 Lark 集成 app 的共享工具箱（sink、入站服务器、配置、加解密） |
| `apps/integrations/linear` | Linear webhook → 飞书通知 + issue 链接预览（Integration） |
| `apps/integrations/github` | GitHub webhook → 飞书通知（Integration） |
| `apps/integrations/x` | 飞书内的 X（Twitter）链接预览（Integration） |
| `apps/automations/meeting-digest` | 自动转写飞书 VC 录制并发送摘要卡片（Automation） |
| `apps/automations/standup-bot` | 每日站会提醒 + 一次性动作（Automation） |

## 控制台特性

- **状态面板**——每个 app 的运行状态 + 最近错误
- **实时事件流**——所有子系统的 `tracing` 事件，SSE 支持 `?since=` / `Last-Event-ID` 回放，持久化到 SQLite（滚动 1 万条）
- **配置编辑**——UI 内 TOML 编辑器，每个 app 有 `enabled` 开关，保存只热重启受影响的 app
- **动作触发**——每个子系统的一次性命令（`linear`/`github`: ping/test-lark、`x`: ping、`standup-bot: announce/ensure/remind/urgent/check`、`meeting-digest: process-meeting`）
- **认证**——通过 **Lark OAuth** 登录，`/api/*` 需要会话。在 `[console]` 绑定 Lark app 前控制台开放，绑定后按 `admins` 白名单限制

## 快速开始

```bash
# 1. 构建
cd dashboard && pnpm install --frozen-lockfile && pnpm build && cd ..
cargo build -p console --release

# 2. 运行（在 UI 中配置 Lark OAuth 前控制台开放）
CONSOLE_SECRET=$(openssl rand -hex 32) \
LINEAR_WEBHOOK_SECRET=your_secret \
LARK_WEBHOOK_URL=https://open.larksuite.com/open-apis/bot/v2/hook/xxx \
./target/release/larkstack-console
# UI 在 http://localhost:8080；linear/github/x 的 webhook 在 :3000/:3001/:3002
# CONSOLE_SECRET（可选）让会话在重启后保持有效；未设置时自动生成并持久化一个密钥。
```

或者用 Docker：

```bash
docker compose up -d
```

完整环境变量见 [docs/deploy/console.md](./docs/deploy/console.md)。

## 独立运行单个 app

每个 app 仍保留独立 `[[bin]]`，可用于本地或单一用途部署（从环境变量读取 `LINEAR_*`、`LARK_*`、`GITHUB_*` …）：

```bash
cargo run -p linear      # 或 github / x / meeting-digest / standup-bot
```

生产环境请部署**控制台**——一个二进制、所有 app、从 UI 切换开关。见 [docs/deploy/console.md](./docs/deploy/console.md) 与 [docs/deploy/railway.md](./docs/deploy/railway.md)。

## 许可证

[MIT](./LICENSE-MIT) 或 [Apache-2.0](./LICENSE-APACHE)，二选一。
