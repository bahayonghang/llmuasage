# 分析看板同步失败横幅与 Claude 重建风险

## Goal

查清用量概览同步命令中心「最近同步存在失败」横幅的根因。让标题、正文和最近一次运行状态描述同一件事。普通同步继续安全；Claude 缺文件的重建风险继续按现有 guard 语义展示。

## Background

2026-08-19 用量概览截图：标题为「最近同步存在失败」，正文为「普通同步安全；重建前需要先处理风险来源。」，最近状态为 `success`，重建风险来源为 `claude`。生成时间 `2026-08-19T11:04:42Z`，最近一次 sync 完成 `2026-08-19T08:58:21Z`。

这是两套独立信号叠在同一块横幅上，不是当前这次 sync 解析失败。

本地只读库与代码对照见 `research/local-evidence.md`。

## Confirmed Facts

1. 最近一次用量导入成功。`run_log` id 178：`sync` / `success` / `2026-08-19T08:58:21Z`。九个来源就绪，parse issue 全 0。扫描 419、新增 145 与 `claude`+`grok` 的 `source_sync_status` 一致。
2. 标题来自 `recent_failures > 0`，不是来自 `last_run`。`src/query/mod.rs` 用最近 10 条**全命令** `run_log`，再过滤 `sync` / `sync --rebuild` / `hook-run`。窗口内 id 176 是 `sync aborted`（`recovered stale running record`，由 2026-08-19 05:12 那次 sync 回收 2026-08-18 未结束的 running 行）。`RunRecord::counts_as_failure` 把 `aborted` 算失败。
3. 正文来自 `lossy_rebuild_risk`，不看失败计数。因此标题是失败文案，正文是重建风险文案。
4. Claude 重建风险成立：728 个 `source_file` 路径在磁盘上不存在，且已有 36,464 条事件。样本是 `~/.claude/projects/.../<uuid>.jsonl`，符合会话 JSONL 轮转。普通 sync 不会删已导入历史；`--rebuild` 仍会被有损重建 guard 挡住。
5. Codex 2564 行、Antigravity 112 行在表里是 `missing`，但路径仍在磁盘上，所以看板不把它们标为重建风险。这是 `source_file` 状态与磁盘不一致，不是本次横幅的直接原因。
6. doctor 现有测试 `doctor_warns_on_recovered_aborted_runs` 把回收后的 `aborted` 当作 warn。命令中心与 doctor 共用 `counts_as_failure`，但文案把该状态说成「同步失败」。

## Requirements

### R1 命令中心标题与最近一次 sync 一致

最近一次 sync 族命令（`sync` / `sync --rebuild` / `hook-run`）为 `success` 时，命令中心不得使用 `syncCenter.headline.failed`。

失败标题只在最近一次 sync 族命令确实失败（`failed`），或当前前台 job 失败时出现。

`recovered stale running record` 的 `aborted` 不得在后续已有成功 sync 时继续把横幅打成「最近同步存在失败」。

### R2 标题与正文描述同一信号

`headline_key` 与 `reason_key` 必须配对：

- 失败标题配失败原因。
- 重建风险标题配重建风险原因。
- 不得再出现「失败标题 + 重建风险正文」。

busy / running / cancelled 保持现有配对。

### R3 失败窗口按 sync 族计算

命令中心的失败计数从 sync 族运行记录取值，不被频繁的 `serve` 启停挤出窗口。不得再用「最近 10 条任意命令」间接决定有没有失败 sync。

### R4 Claude 重建风险保持现有安全语义

Claude 源文件缺失且已有导入事件时，来源卡仍为 `rebuild_risk`。普通同步可点「立即同步」。`--rebuild` 在有损风险下来源上继续拒绝（除非用户明确允许有损重建）。本任务不自动 `forget` 缺失会话文件，不把会话轮转当成同步故障。

### R5 回归覆盖截图组合

测试必须覆盖：窗口内有更早的 `sync aborted`（含 `recovered stale running record`），最近一次 sync 为 `success`，同时存在 Claude 缺文件重建风险。期望：标题为重建风险（或就绪），最近状态为 success，来源卡仍标 Claude 重建风险。不得再输出失败标题。

既有 doctor「回收 aborted 记 warn」行为保持，除非 R1 的分类改动被明确扩展到 doctor（默认不扩展）。

## Acceptance Criteria

- [x] AC1 给定「更早 aborted + 最近 sync success + Claude 缺文件」，命令中心 `headline_key` 不是 `syncCenter.headline.failed`，`last_run.status` 为 `success`，Claude 来源 `lossy_rebuild_risk` 为 true。
- [x] AC2 同一 payload 的 `headline_key` 与 `reason_key` 属于同一信号族（失败/重建风险/就绪/忙碌）。
- [x] AC3 连续多次 `serve` 启停后，命令中心仍能根据最近一次 sync 族记录判定失败与否，而不是被 serve 行挤出窗口。
- [x] AC4 最近一次 sync 族记录为 `failed` 时，标题仍为失败，正文为失败原因。
- [x] AC5 普通 sync 在 Claude 缺文件时仍可启动；有损重建 guard 不回归。
- [x] AC6 现有 sync command center / diagnostics / doctor 相关测试不回归；新增或改写测试钉住 AC1 组合。

## Out of Scope

- 不清理或 `forget` 本地 Claude 已删除会话 JSONL。
- 不修 Codex / Antigravity 表内 `missing` 但磁盘仍在的状态机偏差。
- 不改解析器发现范围、cursor、token 记账。
- 不把 doctor 的 aborted warn 改成 pass（除非后续明确要求）。
- 不改 TUI 文案体系以外的布局或 tokscale 对齐工作。

## Technical Notes

- 判定入口：`Dashboard::sync_command_center_with_diagnostics`（`src/query/mod.rs`）。
- 失败谓词：`RunRecord::counts_as_failure`（`src/store/mod.rs`）。命令中心可改用更窄的谓词，避免改 doctor。
- 回收入口：`RunLog::recover_running_runs`（sync 启动回收 sync/hook-run；serve 启动回收 serve）。
- 重建风险：`load_source_diagnostics` / `lossy_rebuild_risk_with_conn` 用 `Path::exists()`，不是 `state = 'missing'`。
- 文案：`src/web/assets/copy.js` 的 `syncCenter.headline.*` 与 `syncCenter.reason.*`。
- 契约：`.trellis/spec/llmusage/backend/source-sync-contracts.md`。

## Key Decisions

- 按 R1–R5 在本任务实现。用户已确认开始修复。
- doctor / health 的 `counts_as_failure`（含 aborted）保持不变。
- 命令中心失败标题只认最近一次 sync 族记录的 `status == failed`，以及前台 job 失败覆盖。
