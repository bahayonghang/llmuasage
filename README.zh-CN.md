# llmusage

[English](./README.md) · [文档](https://bahayonghang.github.io/llmuasage/zh/)

本地优先的 AI CLI 用量分析工具。`llmusage` 会把本机 Codex、Claude Code、OpenCode、Google Antigravity/Gemini 的本地记录解析进本地 SQLite，然后提供命令行报表、终端 Dashboard、浏览器 Dashboard 和离线 HTML 导出；不上传、不登录、不调用云端用量 API。

> 当前 crate 版本：`0.6.5`。

![llmusage 本地 Web Dashboard 概览](./docs/public/screenshots/web-dashboard-overview.png)

<small>截图来自 `llmusage serve` 启动的脱敏本地 fixture，不是真实用户数据。</small>

## 安装

在当前 checkout 中安装：

```powershell
just install
```

开发时也可以直接运行：

```powershell
cargo run -- --help
```

默认运行时目录是 `~/.llmusage/`。可用 `--home <PATH>` 或 `LLMUSAGE_HOME` 覆盖。

## 最短路径

```powershell
llmusage init
llmusage sync
llmusage
llmusage serve
```

含义：

1. `init` 创建 `~/.llmusage/`、初始化 `llmusage.db`、写入 hook 包装器，并安装支持的本地集成。
2. `sync` 增量解析本地真源，写入 usage 行、30 分钟 bucket、source-file 诊断和行为事实。
3. `llmusage` 显示默认 daily 报表：所选时区下最近 7 个自然日。
4. `serve` 在 `127.0.0.1` 启动本地浏览器 Dashboard。

## 支持的本地来源

| 来源 | 本地记录 |
| --- | --- |
| Codex | OpenAI Codex rollout/session JSONL 与 `config.toml notify` |
| Claude | Claude Code project JSONL 与 `Stop` / `SessionEnd` hooks |
| OpenCode | OpenCode 本地 SQLite 用量库与 `session.updated` plugin event |
| Antigravity / Gemini | Antigravity CLI `Stop` hook（`~/.gemini/config/hooks.json`）、旧 Gemini CLI `SessionEnd` hook，以及旧 `~/.gemini/tmp/*/chats/session-*.json` 导入（`--source gemini` 仍是稳定 id） |

## 常用命令

```powershell
llmusage daily --source codex --since 20260501 --until 20260518
llmusage monthly --breakdown
llmusage session --project my-repo
llmusage blocks --active
llmusage dash
llmusage export html --out .\llmusage-report
```

报表命令只是只读 SQLite 查询；如果数据库过旧，先运行 `llmusage sync`。

## 安全默认值

- 不需要账号登录、device token、上传队列或远端用量 API。
- 普通 `llmusage sync` 遇到原始源文件缺失时会保留已导入 usage。
- `llmusage sync --rebuild` 默认拒绝有损重建，除非同时传入 `--allow-lossy-rebuild`。
- `llmusage diagnostics --forget-file <PATH> --source <SOURCE>` 是显式忽略源文件的写入入口。
- `llmusage doctor --refresh-pricing <file>` 只读取本地价格快照；URL 会被拒绝。

## 文档

- [指南](./docs/zh/guide/getting-started.md)
- [Dashboard](./docs/zh/dashboard/index.md)
- [CLI 参考](./docs/zh/reference/cli.md)
- [安全说明](./docs/zh/safety/index.md)
- [架构说明](./docs/zh/architecture/index.md)

开发门禁：

```powershell
just ci
```
