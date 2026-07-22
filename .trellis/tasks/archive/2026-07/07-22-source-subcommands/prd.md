# 聚焦来源子命令 `<source> <period>`

## Goal

新增 `llmusage <source> <period>` 聚焦写法（如 `llmusage claude daily`），对齐 ccusage 的聚焦视图：
**单来源、无 Agent 列**（"focused views remove the comparison layer"）。数据等价于
`<period> --source <source>`，但呈现层去掉 Agent 对比列。

参考：`ref/repo/ccusage/rust/crates/ccusage-cli/src/cli-commands.json`、`docs/guide/all-reports.md`。

## Background

- ccusage 聚焦：`ccusage claude/codex/opencode/… daily|weekly|monthly|session`，单来源、无 Agent 列。
- **能力矩阵更正**（ccusage）：Claude=daily/weekly/monthly/session/blocks(+statusline)；
  **Codex=daily/monthly/session**；**OpenCode=daily/weekly/monthly/session**；各来源不一致。
- **来源集合差异**：llmusage 来源为 claude/codex/opencode/antigravity，**与 ccusage 的
  gemini/kimi/qwen… 不同**。因此无法 1:1 镜像 ccusage 的 per-source 矩阵——本任务是 llmusage
  自有聚焦 surface（ccusage-inspired），不声称逐来源 parity。
- 现状：llmusage 用 `--source` flag，且 `--source` 在各周期均匀可用；已有单来源渲染
  `render_daily_source_table_styled`（`src/tui/report_table.rs`）可复用为"无 Agent 列"聚焦视图。

## Requirements

### R1. 语法

- 新增顶层来源子命令 `claude`/`codex`/`opencode`/`antigravity`，各挂周期子命令
  `daily`/`weekly`/`monthly`/`session`（`blocks` 见 R4）。参数同顶层同名命令。
- 解析后等价于对应周期命令并注入 `source=<该来源>`。顶层 `--source` 与既有命令保持不变。

### R2. 聚焦呈现（无 Agent 列）

- 聚焦视图为**单来源表，无 Agent 列**（复用/扩展 `render_daily_source_table` 系列，按周期
  参数化）。标题为该来源聚焦标题（非 `Coding (Agent)…/Detected:`）。
- JSON：单来源报表结构（camelCase，形如 C1 行但无 `agents`/无 All 聚合层，或直接单来源 rows —
  实现对齐参考聚焦 JSON，定稿于 design）。

### R3. 能力矩阵（llmusage surface）

- 采用**均匀支持**：4 个来源 × {daily, weekly, monthly, session} 均可用（因 llmusage `--source`
  本就均匀）。**显式声明这是 llmusage 扩展**，不等于 ccusage 的 per-source 矩阵。
- 若某来源在窗口内无数据，走既有空态（不报错）。

### R4. blocks（可选，单列）

- `<source> blocks` 仅在 llmusage 的 blocks 支持按来源过滤时提供；否则本轮不挂 `blocks` 子命令，
  并在 PRD/文档注明（不谎称 parity）。默认**不做** `<source> blocks`，留待明确需求。

### R5. 等价与不变式

- `<source> <period>` 的**数据**与 `<period> --source <source>` 一致；**呈现**去 Agent 列。
- 不改报表 loader/聚合/数据层；纯 CLI 语法 + 聚焦渲染选择（可用共享 args/宏减少样板）。

### R6. 测试

- 解析测试：`claude daily`/`codex monthly`/`opencode weekly` → 正确命令 + source 注入。
- 呈现测试：聚焦视图无 Agent 列、无 Detected 标题；数据与 `--source` 对应 JSON 一致（去 agents 后）。
- 冲突/一致性：位置化来源与顶层 `--source` 的交互（相同值可接受，冲突值报错或按 design）。

## Acceptance Criteria

- [ ] A1：`llmusage claude daily` 输出单来源表、**无 Agent 列**、无 Detected 标题。
- [ ] A2：4 来源 × {daily,weekly,monthly,session} 均可解析并注入 source；数据与 `--source` 等价。
- [ ] A3：能力矩阵在 PRD/help/docs 明确标注为 llmusage 扩展（非 ccusage per-source parity）。
- [ ] A4：顶层 `--source` 与既有命令行为不变；`just ci` 全绿；README 双语+docs 更新。

## Out Of Scope

- 新增来源/别名（gemini/kimi/qwen…）。
- ccusage 来源专属 flag（`--mode`/`--speed`/`--pi-path`/`--start-of-week`/`--open-claw-path`）。
- `<source> blocks`（除非确有需求，默认不做）。

## 依赖

- **依赖 C1（报表/JSON 形状）与 C2（`<source> weekly` 需 weekly 命令存在）**。在 C2 前不挂 weekly
  聚焦子命令；A2 的 weekly 项在 C2 落地后验收（消除此前"无前置却要求 weekly"的矛盾）。
