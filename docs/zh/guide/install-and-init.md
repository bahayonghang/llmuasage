# 安装与初始化

## 从仓库安装

```powershell
just install
```

`just install` 会安装 VitePress 文档依赖，并从当前 checkout 安装 CLI。

开发时不安装也可以直接运行：

```powershell
cargo run -- --help
cargo run -- sync --source codex
```

## 初始化 llmusage

```powershell
llmusage init
```

`init` 是本地设置命令。它会准备运行时目录、初始化数据库、写入 hook wrapper，并安装或探测支持的集成。

## 支持的集成

| 来源 | 集成表面 | 解析的本地数据 |
| --- | --- | --- |
| Codex | `config.toml notify` | OpenAI Codex rollout/session JSONL |
| Claude | `Stop` / `SessionEnd` hooks | Claude Code project JSONL |
| OpenCode | `session.updated` plugin event | OpenCode 本地 SQLite 用量库 |
| Antigravity | `~/.gemini/config/hooks.json` 中的 Antigravity `Stop` hook | 仅记录 hook 触发元数据；没有验证 schema 前不导入 transcript |

如果某个工具没有安装，llmusage 会记录探测/安装状态，并继续处理可见来源。Google 本地 CLI 来源 id 是 `antigravity`；`gemini` 不再作为来源 id。init/uninstall 会 best-effort 清理 llmusage 自己写过的旧 `--source gemini` hook，并保留用户自定义 hook。

## 运行时根目录优先级

运行时根目录按以下顺序解析：

1. `--home <PATH>`
2. `LLMUSAGE_HOME`
3. `~/.llmusage`

示例：

```powershell
llmusage --home .\.tmp-llmusage init
$env:LLMUSAGE_HOME = "D:\tmp\llmusage-home"
llmusage status
```

## 验证设置

```powershell
llmusage status
llmusage doctor
```

`status` 汇总本地数据库和集成状态。`doctor` 默认只读，除非显式传入 `--refresh-pricing <file>`。
