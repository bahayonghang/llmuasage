# Align token accounting with ccusage

## Goal

修正 llmusage 将缓存 token 计入常规使用量所造成的统计偏差，使各平台的
token 归一化与汇总结果以 ccusage 为基准，并与 tokscale 的平台解析语义交叉验证。

## Background

- 用户报告当前 llmusage 会把缓存 token 统计到已使用 token 中，导致结果偏高。
- 对照实现位于 `ref/repo/ccusage` 与 `ref/repo/tokscale`。
- ccusage 是最终兼容基准；tokscale 用于补充 ccusage 未覆盖的平台和字段语义证据。
- 本任务先完成源码审计与规划；在规划评审通过前不修改产品代码。

## Confirmed Facts

- llmusage 只有 Claude、Codex、OpenCode 三个 parser-backed source；Antigravity
  是 integration-only，不应产生推测性的 token 统计。
- ccusage 的 Claude/OpenCode 总量会把 cache read/cache creation 各计一次。
  因此目标不是从所有 `total_tokens` 中全局删除缓存，而是消除 Input、Cache、
  Reasoning 与上游 inclusive total 之间的重复累计。
- 当前 Codex parser 对显式 `cached_input_tokens` 保留完整 raw input，报表又把
  cache read 和 reasoning 加到 total，能够稳定复现双计。
- 当前报表、logs、explorer 和 dashboard 查询混用持久化 total 与组件重算，
  同一数据库可能在不同界面得到不同结果。
- Claude 的流式重复与 sidechain replay、Codex 的复制/归档/fork replay 均需要
  source-aware dedupe；当前 file fingerprint + byte offset event key 不足以对齐参考结果。
- 旧 usage rows 无法仅靠 SQL 可靠修复；现有 `sync --rebuild --source` 具备
  lossy-rebuild guard，是当前可信的重算入口。

## Product Decisions

- 采用 ccusage 语义：cache read/cache creation 从 Input 中分离，并在
  `total_tokens` 中各统计一次；禁止 Input、Cache、Reasoning 与上游 inclusive
  total 重复累计。
- 旧库按 source 显式执行受 lossy-rebuild guard 保护的
  `llmusage sync --rebuild --source <source>`。重建前 legacy source 保持只读并
  显示警告，拒绝写入新口径数据，禁止新旧 token accounting 混用。
- source 文件缺失导致 rebuild guard 拒绝时，保留 legacy 数据并报告 parity
  blocked；不得自动使用 `--allow-lossy-rebuild` 删除不可重建历史。

## Requirements

- 逐平台追踪 llmusage、ccusage 与 tokscale 从原始记录到最终汇总的 token 数据流。
- 明确定义 input、output、cache read、cache creation/write、reasoning 与 total 的规范语义，
  禁止同一 token channel 在默认“使用量”中重复累计。
- `input_tokens` 必须表示非缓存输入；cache read/cache creation 保持独立，并在
  ccusage 统计 total 的平台中各贡献一次。
- `reasoning_output_tokens` 默认是诊断子通道，只有 source contract 证明它与
  output 不重叠时才可参与 total；禁止根据字段存在就直接相加。
- `total_tokens` 由 parser 按 source 语义归一化并持久化，query/UI 只能聚合该字段，
  不得各自重建通用总量公式。
- 对 llmusage 当前已解析的平台逐项给出差异、证据和目标映射；缺少真实语义证据的平台
  不得凭字段名猜测。
- 对 Claude streaming/sidechain 与 Codex copied/fork replay 增加参考兼容的去重规则。
- 修复范围必须覆盖受口径影响的持久化、聚合、成本和用户可见结果，并定义历史数据处理方式。
- 通过 per-source token-accounting version 识别 legacy 数据；只有 source rebuild
  完整成功后才推进版本标记，失败或中断不得伪装成已迁移。
- 保留 llmusage 的本地 SQLite、离线解析与现有 source-aware 架构；不直接依赖参考仓库代码。
- 对外结果以 ccusage 为基准；ccusage 无对应能力时，采用 tokscale 或供应商原始字段语义，
  并明确记录偏差原因。

## Acceptance Criteria

- [x] 形成逐平台 token 字段映射矩阵，包含三套实现的源码锚点与差异结论。
- [x] 定义 source-aware、可测试的规范汇总契约，并区分显示总量、计费输入和缓存子通道。
- [x] 为每个已支持平台列出代表性 fixture/回归用例及预期 token 分量。
- [x] 定义旧数据库兼容与显式 `sync --rebuild --source` 策略，禁止新旧口径混用。
- [x] 定义 CLI JSON、人类可读报表、TUI/Web 与成本结果的对齐验证方式。
- [x] 在实现计划中列出最小相关验证与完整 `just ci` 质量门禁。
- [x] 对共享样本或等价 fixture，llmusage 与 ccusage 的 input、cache create、
  cache read、output、total 必须整数精确一致；token 计数不使用容差。
- [x] 在同一固定费率表下，可比 cost 的绝对误差不超过 `1e-9`；不把 ccusage 的
  live network pricing 当作稳定 CI oracle。
- [x] ccusage 与 tokscale 结果冲突时采用 ccusage，并在研究/测试中显式记录 tokscale 偏差。
- [x] 同一过滤条件在 event、bucket、CLI、JSON、TUI 和 Web 路径返回相同 total。
- [x] Claude/Codex 复制或流式重复样本只计一次，且较完整记录按明确 winner rule 替换旧值。

## Out of Scope

- 新增没有真实样本或明确 token 语义的平台 parser。
- 复制或链接 ccusage/tokscale 的实现作为运行时依赖。
- 与 token 归一化无关的界面重设计或定价目录扩展。

## Planning Artifacts

- Research: `research/token-accounting-comparison.md`
- Technical design: `design.md`
- Execution and validation plan: `implement.md`
