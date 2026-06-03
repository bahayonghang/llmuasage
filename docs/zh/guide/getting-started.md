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

## 2. 初始化本地运行时与 hooks

```powershell
llmusage init
```

`init` 会创建运行时目录、初始化 SQLite、写入 hook 包装器，并在本地配置存在时安装 Codex、Claude Code、OpenCode、Google Antigravity 集成。

默认路径：

| 项目 | 路径 |
| --- | --- |
| 运行时根目录 | `~/.llmusage/` |
| 数据库 | `~/.llmusage/llmusage.db` |
| hook 包装器 | `~/.llmusage/bin/llmusage-hook.cmd`、`~/.llmusage/bin/llmusage-hook.sh` |
| 静态导出 | `~/.llmusage/exports/` |

可用 `--home <PATH>` 或 `LLMUSAGE_HOME` 覆盖运行时根目录。

## 3. 导入本地用量

```powershell
llmusage sync
```

`sync` 会增量解析本地真源，写入标准化 usage 行、30 分钟 bucket、source-file 诊断和行为事实。

只同步单个来源：

```powershell
llmusage sync --source codex
```

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

`serve` 只监听 `127.0.0.1`，会打印本地 URL，并尝试打开默认浏览器。

## 6. 导出离线报告

```powershell
llmusage export html --out .\llmusage-report
```

导出目录包含 `index.html`、`snapshot.json` 和 `assets/*`。

## 下一步

- [第一次同步](./first-sync)：了解安全重建与 NDJSON 进度。
- [第一次报表](./first-report)：了解报表筛选与表格语义。
- [Dashboard](../dashboard/)：了解 `llmusage serve`、行为面板和降级状态。
- [安全说明](../safety/)：了解本地数据路径与破坏性边界。
- [CLI 参考](../reference/cli)：查精确参数。
