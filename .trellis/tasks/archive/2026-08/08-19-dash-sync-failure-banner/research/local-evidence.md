# 本地库与看板横幅对照

采样时间：2026-08-19。库路径：`%USERPROFILE%\.llmusage\llmusage.db`（只读查询）。

截图时间戳：生成 `2026-08-19T11:04:42Z`，最近一次 sync 完成 `2026-08-19T08:58:21Z`。看板版本 `v1.2.0 · local`。页面为用量概览上的同步命令中心。

## 截图与 payload 对照

| 界面 | 值 | 代码来源 |
|------|----|----------|
| 标题「最近同步存在失败」 | `syncCenter.headline.failed` | `query/mod.rs`：`recent_failures > 0` 优先于 rebuild risk |
| 正文「普通同步安全；重建前需要先处理风险来源。」 | `syncCenter.reason.rebuildRisk` | 同函数：`reason_key` 只看 `lossy_rebuild_risk` |
| 最近命令 / 状态 / 完成时间 | `sync` / `success` / `2026-08-19T08:58:21Z` | `last_run` 取最近一条 sync 族命令 |
| 重建风险来源 | `claude` | `diagnostics.by_source[].lossy_rebuild_risk`（磁盘 `Path::exists()`） |
| 扫描 419 / 新增 145 | claude 379+105 + grok 40+40 | `source_sync_status` 求和 |
| 已存 248,936；就绪 9/9 | 九个 parser 来源均有 `stored_events > 0` | 与截图来源卡一致 |

标题与正文来自两套独立条件。最近一次 sync 成功时仍可显示「失败」标题。

## run_log 最近 10 条（`ORDER BY id DESC LIMIT 10`）

命令中心用这 10 条全命令窗口，再过滤 `sync` / `sync --rebuild` / `hook-run`：

| id | command | status | 说明 |
|----|---------|--------|------|
| 185 | serve | running | 当前看板进程，`2026-08-19T11:04:41Z` 启动 |
| 184–179 | serve | aborted | `recovered stale running record`（反复重启 serve） |
| 178 | sync | success | 最近一次用量导入，`08:58:20Z`–`08:58:21Z` |
| 177 | sync | success | 同日 05:12 |
| 176 | sync | aborted | `recovered stale running record`；`2026-08-18T12:40:45Z` 开始，`2026-08-19T05:12:30Z` 被下一次 sync 回收 |

`RunRecord::counts_as_failure`：`status != success && status != running`。id 176 因此计入 `recent_failures = 1`。

`last_run` 取窗口内第一条 sync 族记录，即 id 178 `success`。标题报失败、详情报成功，由此产生。

再重启若干次 serve 后，id 176 会掉出 10 条窗口，标题会变成「检测到重建风险」。该标题依赖 serve 重启次数，而不是最近一次 sync 结果。

`recover_running_runs` 在 `sync` 启动时回收卡住的 `sync`/`hook-run`，在 `serve` 启动时回收卡住的 `serve`。回收写 `aborted` + `recovered stale running record`。doctor 测试 `doctor_warns_on_recovered_aborted_runs` 把该状态当作 warn。

## source_file 与磁盘存在性

重建风险不看 `source_file.state`，只看路径是否还在磁盘上，且该来源已有事件。

| source | live | state=missing | 磁盘上不存在 |
|--------|------|---------------|--------------|
| claude | 266 | 728 | 728 |
| codex | 2567 | 2564 | 0 |
| antigravity | 88 | 112 | 0 |
| 其余 file-backed | 全部 live | 0 | 0 |

只有 claude 的 missing 行对应真实消失的 JSONL。因此来源卡只有 claude 标「重建风险」。

claude missing 样本是 `~/.claude/projects/<project>/<uuid>.jsonl`。部分 `last_seen_at` 为 `2026-08-19T05:12:30Z`，`last_state_change_at` 为 `2026-08-19T08:58:21Z`：当天第二次 sync 未再发现这些会话文件。这与 Claude Code 会话 JSONL 轮转/删除一致。已导入的 36,464 条 claude 事件仍在库中。普通 sync 不会删除它们；`--rebuild` 在缺文件时会被 guard 拒绝。

codex missing 行在磁盘上仍存在。样本为 `~/.codex/sessions/2026/05/12/rollout-*.jsonl`。2564 行的 `last_seen_at` 均为 `2026-08-17T06:13:42.649Z`，`last_state_change_at` 均为 `2026-08-18T12:26:47Z`。这是一次发现集变化后的状态机标记，不是文件丢失。看板因此不把 codex 标为重建风险。

antigravity missing 行是 `~/.gemini/tmp/.../chats/session-*.json`，`last_seen_at` 停在 `2026-05-27`，状态改 missing 于 `2026-08-16`（Antigravity 改为 CLI `conversations/*.db` 之后）。文件仍在磁盘上，当前 parser 不再发现它们。

## 相关表面

- `health()` / `diagnostics()` 的 `recent_failures` 对最近 10 条**全命令**做 `counts_as_failure`，不过滤 sync 族。当前窗口会把多次 `serve aborted` 算进去。洞察卡 `sync_failure` 使用该列表。
- 同步命令中心只统计 sync 族，但仍用同一 10 条全命令窗口，因此窗口里的 sync 条数随 serve 重启被挤掉。
- `api_dashboard_embeds_sync_command_center_contract` 插入的是较新的 `failed` 记录，不覆盖「后来 success、窗口里仍有 aborted」的标题/正文组合。
