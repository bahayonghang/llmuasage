# Journal - lyh (Part 1)

> AI development session journal
> Started: 2026-06-05

---



## Session 1: Optimize serve dashboard range switching

**Date**: 2026-06-05
**Task**: Optimize serve dashboard range switching
**Package**: ccexplorer
**Branch**: `dev`

### Summary

Implemented fast range switching for llmusage serve with core dashboard scope, live request cache/coalescing, stale secondary refresh UI, focused tests, CI, and browser verification.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `065fc4d` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 2: Bootstrap Trellis Guidelines

**Date**: 2026-06-12
**Task**: Bootstrap Trellis Guidelines
**Package**: ccexplorer
**Branch**: `dev`

### Summary

Committed project cleanup, Trellis workflow metadata, and source-backed bootstrap guideline specs, then archived the bootstrap task.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `0f26c83` | (see git log) |
| `9cca110` | (see git log) |
| `7bf6617` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 3: Complete tokscale collection and TUI migration

**Date**: 2026-06-12
**Task**: Complete tokscale collection and TUI migration
**Package**: llmusage
**Branch**: `dev`

### Summary

Implemented monitor-only source descriptors, skipped-file sync stats, tokscale-style TUI affordances, docs/spec updates, and completed full just ci validation.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `0b4d81f` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 4: Optimize dash overview and warning styling

**Date**: 2026-06-12
**Task**: Optimize dash overview and warning styling
**Package**: llmusage
**Branch**: `dev`

### Summary

Committed the 0.8.0 version sync, enriched the terminal dash overview, styled the deprecated tui warning, archived the Trellis task, and recorded validation.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `790ff1a0428dbee8f8b2449c1bf1a301ec162b3e` | (see git log) |
| `b696248bdd7616be1005e17332cba61f10927659` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 5: Restore source-status CLI command

**Date**: 2026-06-14
**Task**: Restore source-status CLI command
**Package**: llmusage
**Branch**: `dev`

### Summary

Restored the documented source-status command, shared status rendering with status, updated CLI help/docs, and verified focused/full gates.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `617baa3` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 6: Document codex-tracer usage

**Date**: 2026-06-16
**Task**: Document codex-tracer usage
**Branch**: `dev`

### Summary

Added codex-tracer docs in English/Chinese, documented the embedded schema.sql contract, and archived the documentation subtask.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `28e5a06` | (see git log) |
| `28abf32` | (see git log) |
| `bd7e222` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 7: 吸收 AIUsage 调用分析能力：OpenCode 工具解析 + 零调用检测

**Date**: 2026-06-21
**Task**: 吸收 AIUsage 调用分析能力：OpenCode 工具解析 + 零调用检测
**Branch**: `dev`

### Summary

为 behavior 补齐 OpenCode part 表工具/MCP/skill 调用解析（归一化 UsageToolCall，关联 messageID/sessionID，幂等写入），并把 Claude Skill 名细分到 input.skill；新增 query/inventory 模块探测三家已装技能（SKILL.md）与 MCP 配置（JSON/TOML），Dashboard::zombie_report 按来源与已用集合求差标出僵尸候选，接入 TUI Optimize 只读建议区。CI 三关通过（fmt/clippy/test 332 passed）。明确排除 AIUsage 的 proxy/配额查询（违反本地只读）。Codex skill、成功率耗时、Web 渲染列为未来项。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `b205b34` | (see git log) |
| `5cd63d8` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 8: Token 统计口径增强与 TUI 观感升级 (A+B)

**Date**: 2026-07-01
**Task**: Token 统计口径增强与 TUI 观感升级 (A+B)
**Branch**: `dev`

### Summary

对标 ref/token-tracker：A) context window 利用率(查询期计算+pricing catalog 窗口)、longest streak、session gap-capped active/span；B) 多主题系统(default_dark 零回归+catppuccin_mocha, t 键/env 切换)、GitHub 7×N 热力图网格+分位分档、Models/Cost/Sources 长尾折叠、Blocks(burn-rate) 第 9 面板。354 测试通过，clippy/fmt 全绿，默认渲染零回归。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `3d3e202` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 9: provider_label 用量归因维度

**Date**: 2026-07-02
**Task**: provider_label 用量归因维度
**Branch**: `dev`

### Summary

实现 provider_label schema v14、CCR provider map sync 归因、回归测试与 ADR

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `af58f0a` | (see git log) |
| `b7d06be` | (see git log) |
| `14bd284` | (see git log) |
| `6d4d0d2` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 10: Claude Fable/Mythos 模型统计覆盖

**Date**: 2026-07-03
**Task**: Claude Fable/Mythos 模型统计覆盖
**Branch**: `dev`

### Summary

为 Claude Fable 5 和 Claude Mythos 5 添加 static-v1 定价、OpenCode/Anthropic 匹配、1M context window、成本与 context pressure 回归测试，并记录 pricing catalog 维护规格。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `311e9bc` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 11: Optimize serve number formatting

**Date**: 2026-07-06
**Task**: Optimize serve number formatting
**Branch**: `dev`

### Summary

Created and completed Trellis task 07-06-serve-number-format. Added shared compact token formatting for the serve dashboard, updated model/source/project/trend/cost/explorer renderers to show K/M/B/T labels with exact-value tooltips, and verified with JS syntax checks, cargo fmt, clippy, focused asset test, full cargo test, git diff --check, and a live serve asset/API smoke.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `3e2845a` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 12: 完成可配置模型目录与 GPT-5.6 支持

**Date**: 2026-07-10
**Task**: 完成可配置模型目录与 GPT-5.6 支持
**Branch**: `dev`

### Summary

实现内置基础目录与用户覆盖层的双层配置，新增 catalog 管理命令、持久化激活与失败恢复；补充 GPT-5.6 Luna、Terra、Sol 的定价、上下文和来源匹配，并同步中英文文档、契约与回归测试。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `b1aa754` | (see git log) |
| `f3db2c0` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 13: 优化 Explorer 时间序列展示

**Date**: 2026-07-11
**Task**: 优化 Explorer 时间序列展示
**Branch**: `dev`

### Summary

将 Cost Explorer 超长时间序列表重构为最多 5 个独立刻度趋势小图，并提供默认折叠、限高滚动的最近 80 条明细；完成中英文、明暗主题、桌面移动端 Chrome 验收及 just ci。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `8e75049` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 14: 优化看板时间范围切换性能

**Date**: 2026-07-11
**Task**: 优化看板时间范围切换性能
**Branch**: `dev`

### Summary

新增精简交互投影、真实取消和聚合查询路由；代表库四档 API p95 均低于 400 ms，并完成 0.9.1 版本同步。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `257ecd5` | (see git log) |
| `c0873b0` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 15: Align token accounting with ccusage

**Date**: 2026-07-16
**Task**: Align token accounting with ccusage
**Branch**: `dev`

### Summary

Aligned Claude, Codex, and OpenCode token normalization with ccusage; made persisted totals authoritative across queries and UI; added guarded per-source accounting-version rebuilds, parity tests, documentation, and durable contracts.

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `0848fe8` | (see git log) |
| `3702195` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 16: 优化同步数据库初始化性能与进度可见性

**Date**: 2026-07-16
**Task**: 优化同步数据库初始化性能与进度可见性
**Branch**: `dev`

### Summary

将定价桶对账改为线性主键集合比较，新增人类输出、NDJSON 生命周期事件和结构化日志，并以 53.9 万事件快照验证性能与一致性；同步更新中英文文档。

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `731c91f77d3ab9b263b18037c0064a60db46cecc` | (see git log) |
| `89bc7de1d07cfcafea3cdbfc38935bc2ec74ace3` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 17: Serve 安全迁移旧版 token 统计

**Date**: 2026-07-17
**Task**: Serve 安全迁移旧版 token 统计
**Branch**: `dev`

### Summary

实现 serve 启动前按来源安全重建 legacy token accounting，保留有损来源与 parserless 历史，并修正 full rebuild 的逐源边界；补齐回归测试、双语文档和 Trellis 规范。

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `31ca870` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 18: 完成首页概览与多来源同步性能优化

**Date**: 2026-07-19
**Task**: 完成首页概览与多来源同步性能优化
**Branch**: `dev`

### Summary

完成 home_overview 共享 row stream 与 diagnostics 缺失源优化，恢复 80ms 门；完成 Claude/Codex/OpenCode 增量扫描与写入性能修复。debug/release 80ms 各连续三次通过，严格 Clippy、未设置 CI=1 的完整串行测试、docs build 和 git diff check 全绿；保留用户原有配置与 README WIP。

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `66d596f` | (see git log) |
| `05add10` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 19: sync 进度条系统、摘要表格与全链路 profiling

**Date**: 2026-07-20
**Task**: sync 进度条系统、摘要表格与全链路 profiling
**Branch**: `dev`

### Summary

indicatif 进度条（OpenCode spinner / Codex/Claude 重放文件确定条）+ RAII 终端清理 + Ctrl-C 取消 + LLMUSAGE_PROGRESS=off；对齐摘要表格（bytes/parse/write 列，TTY 着色）；profiling 证明渲染开销 0.009%、通道零丢弃，冷跑 Codex write 25.3s 另立 P3 backlog（07-20-sync-cold-import-write-throughput）。全量 424 测试绿、fmt/clippy 净。

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `a16f55c` | (see git log) |
| `36e19a1` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 20: 归档 sync 冷跑全量导入写入吞吐 backlog

**Date**: 2026-07-20
**Task**: 归档 sync 冷跑全量导入写入吞吐 backlog
**Branch**: `dev`

### Summary

按用户要求归档未进入实施的 07-20-sync-cold-import-write-throughput 规划任务；未关联工作提交，保留其他 TUI 规划与 TODO.md 的未跟踪改动。

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

(No commits - planning session)

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 21: 补齐 TUI 首访渲染线程证据并归档

**Date**: 2026-07-21
**Task**: 补齐 TUI 首访渲染线程证据并归档
**Branch**: `dev`

### Summary

新增 release 忽略基准测量 Stats、Behavior、Blocks 首访渲染线程四个同步区段，记录三次中位数并将父任务 X7(a) 更新为通过；just ci 全绿，随后归档 benchmark 子任务与 TUI 优化父任务。保留用户未跟踪 TODO.md 未暂存。

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `75b9f5a` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete
