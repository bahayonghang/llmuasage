# 快速开始

如果你只想建立本地数据库、看到第一份报表并打开浏览器 Dashboard，从这里开始即可。

## 环境要求

- Rust stable toolchain
- Node.js 20+
- npm 10+
- `just`

## 1. 从当前 checkout 安装

```powershell
just install
```

该任务会安装 `docs/` 下的 VitePress 依赖，并通过 `cargo install --path . --locked --force` 安装 CLI。

## 2. 初始化本地运行时

```powershell
llmusage init
```

`init` 会创建运行时目录并初始化 SQLite，不会安装 hook/plugin，也不会修改 Codex、Claude Code、OpenCode 或 Antigravity 配置。

默认路径：

| 项目 | 路径 |
| --- | --- |
| 运行时根目录 | `~/.llmusage/` |
| 数据库 | `~/.llmusage/llmusage.db` |
| 静态导出 | `~/.llmusage/exports/` |

可用 `--home <PATH>` 或 `LLMUSAGE_HOME` 覆盖运行时根目录。

## 3. 导入本地用量

```powershell
llmusage sync
```

`sync` 会增量解析本地真源，写入标准化 usage 行、30 分钟 bucket、source-file 诊断和行为事实。

sync 只被动读取。Antigravity CLI conversations 由已注册 parser 导入。hook 时代的 Antigravity 行仍可查询；这些行没有文件归属时，rebuild 会被拒绝。

如果这台机器曾使用会安装 hook 的旧版 llmusage，请执行一次 `llmusage uninstall`，清理 llmusage 自有的遗留 hook、plugin 和 wrapper；已有用量数据不会被删除。

只同步单个来源：

```powershell
llmusage sync --source codex
```

从你已控制的另一台机器导入用量：

```powershell
llmusage remote add devbox user@devbox
llmusage sync
```

远端需要安装兼容的 `llmusage` 二进制。解析仍发生在那台机器上。`llmusage sync` 通过 SSH 拉取规范化 shard。单台不可达主机会被跳过，不导致本地 sync 失败。

## 4. 查看默认报表

```powershell
llmusage
```

没有子命令时，`llmusage` 等价于 `daily`，显示所选时区下最近 7 个自然日（包含今天）。`--timezone local` 使用本机当前固定本地偏移；如果需要跨机器可复现的历史分组，请显式传入 `--timezone +08:00` 这类固定偏移。

自动化场景使用 JSON：

```powershell
llmusage daily --json --source antigravity
```

## 5. 打开本地 Dashboard

终端 Dashboard：

```powershell
llmusage dash
```

浏览器 Dashboard：

```powershell
llmusage serve
```

`serve` 默认监听 `127.0.0.1`，会打印本地 URL，并尝试打开默认浏览器。远程服务器可使用 `serve --public --no-open --port 37421`，但必须配合防火墙、SSH 隧道或反向代理：Dashboard 不提供认证和 TLS。

Codex 专属浏览器 Dashboard：

```powershell
llmusage codex-tracer
```

当你需要独立的 `codex-tracer.db` 以及 Codex 专属调用/线程细节时，使用这个命令。

## 6. 导出离线报告

```powershell
llmusage export html --out .\llmusage-report
```

导出目录包含 `index.html`、`snapshot.json` 和 `assets/*`。

## 下一步

- [第一次同步](./first-sync)：了解安全重建与 NDJSON 进度。
- [第一次报表](./first-report)：了解报表筛选与表格语义。
- [Codex Tracer](./codex-tracer)：了解专用 Codex Dashboard 与重建行为。
- [Dashboard](../dashboard/)：了解 `llmusage serve`、行为面板和降级状态。
- [安全说明](../safety/)：了解本地数据路径与破坏性边界。
- [CLI 参考](../reference/cli)：查精确参数。
