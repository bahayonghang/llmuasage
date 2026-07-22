# 修复 daily cache 统计并优化汇总展示

## Goal

修复 `llmusage daily` 人类可读表格中 cache 相关分量与 `Total Tokens` 的展示口径，
使表格总量能由可见分量直接核对并与 `ref/repo/ccusage` 的表格语义一致；同时将
表格中的大数改为紧凑单位，并增强 `Total` 汇总行的可读性。

## Background

- 用户提供了 `2026-07-17` 至 `2026-07-18` 的 `ccusage daily` 与
  `llmusage daily` 对照输出。
- 对照中的 cache create 日汇总实际完全相同：`2026-07-17` 为 `7,847,755`，
  `2026-07-18` 为 `3,842,949`。OpenAI/Codex 原始用量只提供 cached input（cache
  read），没有独立 cache-write 字段，因此 Codex `Cache Create = 0` 是来源语义。
- 稳定样本 `2026-07-17` 的四个可见 token 分量在两边完全相同，但 llmusage 的
  Codex `Total Tokens` 多出 `76,820`；该差额来自持久化的权威总量中未在统一表格
  展示的 reasoning/extra token，而非 cache create 重复计数。
- `ref/repo/ccusage/rust/crates/ccusage/src/adapter/all/report.rs:444` 的统一表格明确用
  input + output + cache create + cache read 重算表格总量；其 JSON 仍保留来源自己的
  权威 `totalTokens`。本任务采用同样的 presentation projection。
- Claude 成本差异属于另一条定价链：当前 llmusage 静态目录会把
  `claude-opus-4-8`/`claude-sonnet-5` 命中旧 family 价格，且未保留 5m/1h cache-write
  明细；它不是 cache create token 统计错误。
- 第二轮同一时刻 JSON 对照发现 `2026-07-18 / gpt-5.6-sol` 在 llmusage 中比 ccusage
  多 `75,064` input、`381,440` cache read、`3,629` output。真实 rollout 中一个带
  `thread_spawn`/`forked_from_id` 的 fork 文件把父会话历史改写到创建秒；llmusage
  未跳过这 10 条 replay，三项差额精确等于该前缀之和。
- `ref/repo/ccusage` 会识别 replay 文件，并在保留累计 token baseline 的同时跳过
  创建秒内的父历史。本任务补齐该 parser 语义，并提升 Codex 单源 accounting marker，
  使已有数据库必须经过受保护的 Codex rebuild 才会标记为 current。

## Requirements

- 追踪 daily 报表中 input、output、cache create、cache read 与 total tokens 从
  SQLite 查询 DTO 到终端表格的展示链路，并保留证据化根因。
- unified/focused 人类可读报表中，同一行的 `Total Tokens` 必须等于该行展示的
  input + output + cache create + cache read；不得把隐藏 reasoning/extra 分量混入。
- CLI JSON、内部查询 DTO、SQLite `total_tokens` 和其他 dashboard/export/TUI 消费者
  继续保留来源权威总量，不因文本兼容投影而改变。
- 表格中的 token 数字使用稳定的紧凑单位（例如 `K`、`M`、`B`），不再显示带
  千位分隔符的完整原始大数。
- `Total` 汇总行前使用不依赖颜色的强化分隔线，使其与普通日期/Agent 明细行有
  明确的视觉层级，同时保持非交互终端和测试快照中的确定性。
- 为统计修复、边界单位格式和 Total 展示添加聚焦回归测试。
- Codex parser 必须识别带 `thread_spawn` 或 `forked_from_id` 的 replay rollout；仅当
  前两个有效 token 快照位于同一秒时，跳过该秒内的 replay 前缀，并保留最后一个
  cumulative total 作为后续增量基线。
- replay 修复只提升 Codex 的 token-accounting 版本；Claude/OpenCode 继续使用现有
  版本，普通 sync 必须拒绝向旧 Codex 数据追加，直到执行受保护的单源 rebuild。
- 使用隔离的临时 llmusage 数据库读取本机 Codex 源，验证修复后两日模型分量与
  同一时刻 ccusage 结果一致；未经用户明确授权不重建真实用户数据库。
- 保留现有 `Cargo.toml` 与 `Cargo.lock` 中用户未提交的 `1.0.1` 版本改动，不纳入
  本任务修改。

## Acceptance Criteria

- [x] 对用户提供的两天样本或等价夹具，表格 `Total Tokens` 可由四个可见 token
      分量复算，修复前包含隐藏 reasoning/extra 的错误有回归测试覆盖。
- [x] daily 的日期汇总、Agent 明细和最终汇总使用同一套 token 聚合规则，不出现
      `Total Tokens` 无法由该行报表分量解释的情况。
- [x] 所有 token 数值列都以 `K`/`M`/`B` 等紧凑单位显示，并覆盖零值、单位边界、
      舍入和十亿级数值测试。
- [x] `Total` 汇总行前存在强化分隔线，在不依赖颜色的输出中也能与普通行清楚
      区分，列对齐保持稳定。
- [x] `--json` 的 `totalTokens` 仍返回持久化的来源权威总量，证明修复仅影响文本投影。
- [x] 聚焦测试通过，随后通过与改动风险相称的 Rust 格式、lint 和测试门禁。
- [x] 最终 diff 不包含对既有版本提升的覆盖或无关重构。
- [x] `gpt-5.6-sol` replay 前缀回归夹具精确排除 input/cache read/output，并保留
      cumulative baseline；普通同秒事件不被误删。
- [x] Codex 旧 accounting marker 被识别为 legacy，Claude/OpenCode marker 仍保持
      current，普通 sync 给出 Codex 单源 rebuild 指引。
- [x] 临时数据库完整重放后，`2026-07-17`、`2026-07-18` 的 Codex 模型分量与当前
      ccusage JSON 对照一致。
- [x] 新 parser/accounting 代码通过聚焦测试、串行全测试与 `just ci`。

## Out of Scope

- 未经用户明确授权修改原始用户数据库或执行真实数据 rebuild。
- 修复 Claude 新模型定价、5m/1h cache-write 成本或历史成本；这些需要独立的定价
  目录、持久化 schema、accounting marker 与 rebuild 设计。
- 修改 SQLite schema、内部查询 DTO 或 CLI JSON 的权威 `total_tokens` 语义。
- 改动用户现有的版本号提交内容。
