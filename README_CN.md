# Lark Stack - 开源的飞书应用中心

**面向 Lark / 飞书的开源集成中枢**——用一个自托管二进制，构建匹敌 Slack 级别的生态。

一个进程监管所有集成与自动化，自带 React 控制台（状态、配置、动作触发）。

![Rust](https://img.shields.io/badge/Rust-Edition_2024-orange.svg)
![License](https://img.shields.io/badge/License-MIT-blue.svg)

[![README in English](https://img.shields.io/badge/English-d9d9d9)](./README.md)
[![简体中文 README](https://img.shields.io/badge/简体中文-d9d9d9)](./README_CN.md)

<hr>

## 为什么做 Lark Stack

Slack 赢得创业团队靠的是生态，而不是聊天本身。数千个集成意味着团队日常用的每个工具都会出现在他们对话的地方——一个 PR、一次部署、一笔到账的账单、一次 on-call 呼叫、一条 CRM 更新。

但 Lark 的集成目录还很薄，尤其在中国以外——选择它往往意味着放弃这套生态。

**Lark Stack 来补这道缺口。** 一个自托管、开源的中枢，把缺失的 Slack 级集成打包进单个、可在 Web 控制台开关的二进制。而且不止于集成：它同样承载在 Lark 内自主运行的 Automation（站会、会议纪要……）——把生态补宽，也补深。靠 `App`/`Instance` 契约加 `lark-kit`，每个新 app 都是一个小而自洽的 crate。

目标：创业公司或企业可以选择 Lark 而不失去任何东西——依赖的每个外部工具都桥接进来，重复的每项流程都交给内置的 Automation，全在团队已经工作的地方。

## Apps

App 是控制台监管和开关的可插拔单元。**Integration** 把外部系统桥接进 Lark；**Automation** 按计划或事件自主运行。点击名称查看其文档。

| App | 类型 | 功能 |
| :--- | :--- | :--- |
| [`Linear`](./apps/integrations/linear) | ![Integration][kind-integration] | Linear webhook → 飞书通知卡片 + issue 链接预览 |
| [`GitHub`](./apps/integrations/github) | ![Integration][kind-integration] | GitHub webhook → PR/issue/CI/安全告警卡片 + review 请求私信 |
| [`X`](./apps/integrations/x) | ![Integration][kind-integration] | X（Twitter）链接预览，渲染为飞书卡片（仅预览） |
| [`Minutes`](./apps/automations/minutes) | ![Automation][kind-automation] | 自动转写飞书 VC 录制（STT）→ 摘要卡片 + 可选飞书文档 |
| [`Standup`](./apps/automations/standup) | ![Automation][kind-automation] | 每日站会提醒 + 一次性命令（announce/remind/urgent/check） |

[kind-integration]: https://img.shields.io/badge/Integration-2563eb?style=flat-square
[kind-automation]: https://img.shields.io/badge/Automation-16a34a?style=flat-square

三个 integration 都挂在控制台端口下的 `/webhooks/<app>/`（如 `/webhooks/linear/webhook`、`/webhooks/github/webhook`、`/webhooks/x/lark/event`）——不再有独立端口。

## 路线图

上面的桥接只是起点；目标是覆盖团队对 Slack 应用市场所期待的各个品类。想要某个集成早点到、或者列表里没有？[提个 issue](../../issues) 或直接发 PR：一个新集成就是一个自洽的 crate。

| 品类 | 已发布 | 规划中 |
| :--- | :--- | :--- |
| 开发与代码 | Linear、GitHub | GitLab、Jira、Sentry |
| CI/CD 与部署 | GitHub CI | Vercel、Netlify、通用 webhook |
| 故障与 on-call | — | PagerDuty、Opsgenie、incident.io |
| 可观测性 | — | Datadog、Grafana、Alertmanager |
| 营收与增长 | — | Stripe、HubSpot |
| 客户支持 | — | Zendesk、Intercom |
| 社交与信息流 | X / Twitter | RSS、状态页 |
| 团队仪式 | 站会 | 投票、日历摘要、复盘 |

## 框架

| Crate | 作用 |
| :--- | :--- |
| `crates/larkstack-core` | 插件契约（`App`/`Instance`/`Manifest`）+ 控制面（`ControlPlane`、`EventStore`） |
| `crates/larkstack` | 框架宿主——按 app 监管 + axum API + 内嵌 React UI |
| `crates/console` | 瘦二进制 `larkstack-console`——注册内置 apps 并运行宿主 |
| `crates/lark-kit` | 各 Lark 集成 app 的共享工具箱（sink、入站服务器、配置、加解密） |

## 控制台特性

- **状态面板**——每个 app 的运行状态 + 最近错误
- **实时事件流**——所有子系统的 `tracing` 事件，SSE 支持 `?since=` / `Last-Event-ID` 回放，持久化到 SQLite（滚动 1 万条）
- **配置编辑**——UI 内 TOML 编辑器，每个 app 有 `enabled` 开关，保存只热重启受影响的 app
- **动作触发**——每个子系统的一次性命令（`linear`/`github`: ping/test-lark、`x`: ping、`standup: announce/ensure/remind/urgent/check`、`minutes: process-meeting`）
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
# UI + API + webhook 都在 http://localhost:8080（webhook 路径为 /webhooks/<app>/）
# CONSOLE_SECRET（可选）让会话在重启后保持有效；未设置时自动生成并持久化一个密钥。
```

或者用 Docker：

```bash
docker compose up -d
```

完整环境变量见 [docs/deploy/console.md](./docs/deploy/console.md)。

## 独立运行单个 app

automation 类 app（minutes/standup）保留独立 `[[bin]]`，可用于本地/CLI（从环境变量读取 `LARK_*`、`STANDUP_*` …）。integration（linear/github/x）是库，统一通过控制台运行。

```bash
cargo run -p standup     # 或 minutes
```

生产环境请部署**控制台**——一个二进制、所有 app、从 UI 切换开关。见 [docs/deploy/console.md](./docs/deploy/console.md) 与 [docs/deploy/railway.md](./docs/deploy/railway.md)。

## 许可证

[MIT](./LICENSE-MIT) 或 [Apache-2.0](./LICENSE-APACHE)，二选一。
