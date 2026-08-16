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

## 更新已安装版本

自更新命令使用 Git 和本机 Rust/Cargo 工具链解析、构建并安装官方更新目标：

```powershell
llmusage update --check
llmusage update
llmusage update dev
```

默认 `main` 渠道表示最高的稳定语义版本 release tag。llmusage 会从官方仓库
解析 tag 与 commit，显示两者和准确安装命令，然后请求确认。确认后会再次解析
目标；目标发生变化时立即停止。稳定渠道安装锁定到已显示的不可变 commit：

```powershell
cargo install --git https://github.com/bahayonghang/llmuasage llmusage --rev <resolved-sha> --locked --force
```

`--check` / `-c` 会联网解析官方 refs 并在预览后退出，绝不启动 Cargo。只有明确
需要尚未发布的改动时才使用 `dev`：预览会显示当前 dev commit，但 Cargo 仍会
跟踪可变的 `dev` 分支，该分支可能继续变化或暂时无法构建。

## 初始化 llmusage

```powershell
llmusage init
```

`init` 是本地设置命令。它只准备运行时目录并初始化数据库，不会写入第三方配置。

## 被动来源

| 来源 | 解析的本地数据 | 状态 |
| --- | --- | --- |
| Codex | OpenAI Codex rollout/session JSONL | 被动 parser |
| Claude | Claude Code project JSONL | 被动 parser |
| OpenCode | OpenCode 本地 SQLite 用量库 | 被动 parser |
| Kimi Code | turn-scoped `usage.record` 行 | 被动 parser |
| Pi / Oh My Pi | 两个支持目录中的 session JSONL | 被动 parser |
| Grok Build | 会话根目录 sidecar | 被动 parser（`total_only`） |
| Antigravity | CLI `conversations/*.db`，并保留 hook 时代历史行 | 被动 parser；存在未归属历史时拒绝 rebuild |
| ZCode | `cli/db/db.sqlite` 的 `model_usage` completed 行 | 被动 parser |
| DeepSeek Harness | `sessions/**/session.jsonl(.zstd)` | 被动 parser |

Google 本地 CLI 来源 id 仍是 `antigravity`；`gemini` 不作为来源 id。从会安装 hook 的旧版本升级后，应执行一次 `llmusage uninstall`。清理只移除 llmusage 自有的遗留命令、plugin 和 wrapper，保留同级用户配置与历史备份；除非传入 `--purge`，否则不会删除用量数据库。

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

`status` 汇总本地数据库和来源状态。`doctor` 默认只读，除非显式传入 `--refresh-pricing <file>`。
