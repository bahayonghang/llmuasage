# 本机 hooks 安装现状排查与清理记录（2026-07-27）

## 排查结果：本机实际安装面

| 工具 | 安装情况 | 详情 |
| --- | --- | --- |
| Claude Code（`~/.claude/settings.json`） | **已安装，且重复** | `hooks.Stop` 与 `hooks.SessionEnd` 各 2 条 llmusage 条目，共 4 条。两条格式不同：`cmd /c "C:\...\llmusage-hook.cmd --source claude ..."` 与 `cmd /c ""C:\...\llmusage-hook.cmd" --source claude ..."`（路径多一层引号）——不同版本安装器写入的格式变体，旧条目未被新版本清掉 |
| hook 包装脚本 | **已安装** | `~/.llmusage/bin/llmusage-hook.cmd` / `.sh`，内容为调用 `llmusage.exe hook-run %*` |
| Codex（`~/.codex/config.toml`） | 未安装 | `notify` 指向 codex-computer-use，与 llmusage 无关；文件中的 "llmusage" 匹配全部是本仓库目录 Trellis hooks 的信任哈希（`D:\...\llmusage\.codex\hooks.json`），不是 llmusage 安装物 |
| OpenCode（`~/.config/opencode/`） | 未安装 | plugins/ 下只有 herdr-agent-state.js、rtk.ts |
| Antigravity（`~/.gemini/antigravity-cli/`） | 未安装 | settings.json 仅 trustedWorkspaces 含本仓库路径；brain/ 下匹配全是会话转录 |

## 关键缺陷证据（影响任务设计）

`llmusage uninstall`（`src/commands/uninstall.rs` → `integrations::uninstall_all`）对 Claude 条目的摘除走 `remove_event_command`（`src/integrations/claude.rs:225`），按 `HookTarget::current(app).shell_command(...)` 生成的**当前格式字符串精确相等**匹配。本机存在两种历史格式变体，精确匹配只能摘掉与当前版本格式一致的那条，**另一条会残留**。任务中的卸载/清理路径必须改为按"命令串包含 llmusage hook 标识"的宽匹配（或按 `llmusage-hook` 文件名匹配），并覆盖历史格式变体的测试。

## 已执行的本机清理（2026-07-27，手动完成）

1. 备份：`~/.claude/settings.json` → `~/.claude/settings.json.bak-before-llmusage-hook-cleanup`。
2. 从 `hooks.Stop` 移除 2 条、`hooks.SessionEnd` 移除 2 条 llmusage 条目（按命令串含 `llmusage` 宽匹配）；`SessionEnd` 因摘空整键删除；rtk/orca/formatter 等其他 hook 未动；回写后 JSON 校验通过。
3. 删除 `~/.llmusage/bin/llmusage-hook.cmd`、`llmusage-hook.sh`；`bin/` 摘空后删除目录。
4. **保留** `~/.llmusage/` 其余内容（SQLite 用量库、pricing、backups 等）——用户继续用 `sync` 被动读取。
5. Codex/OpenCode/Antigravity 无安装物，无需处理。

## 对任务范围的推论

- 本机清理已完成，任务中的"卸载已安装 hooks"能力仍需保留或一次性提供（其他机器/其他用户升级时需要），且必须修复精确匹配缺陷。
- `hook-run` CLI 命令（`src/commands/mod.rs:200`，hidden）、`Init` 的 integration 安装路径、`~/.llmusage/bin` 包装脚本生成逻辑均属移除面。
- `uninstall --purge` 语义（删除整个 `~/.llmusage`）与本任务无关，注意不要误触。
