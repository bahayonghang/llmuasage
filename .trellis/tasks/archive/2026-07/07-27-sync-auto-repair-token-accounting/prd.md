# 同步自动修复旧 token accounting

## Goal

让普通 `llmusage sync` 在检测到所选 parser-backed 来源仍使用旧 token
accounting 合约时，先明确告知用户即将进行安全修复，再自动完成可无损重建的
来源迁移并继续本轮同步，免去逐个复制执行 `sync --rebuild --source ...`。

自动化不得削弱现有数据保护：任何可能丢失不可重建历史的情况仍需在修改数据库
前拒绝，并保留清晰的人工恢复路径。

## Background

- 用户现场日志显示 schema v17 `add_source_sync_parse_issues` 已完成，随后普通
  `sync` 才被 token accounting 写入 guard 拒绝；这不是 schema migration
  失败，而是独立的 per-source accounting marker 升级编排缺口。
- 当前 Codex 的期望 marker 是 `3`，Claude、OpenCode、Kimi Code、Pi 与 Grok
  是 `2`。来源已有历史行但 marker 缺失或不等于当前值时即为 legacy。
- 普通 sync 目前只报错并要求用户逐源显式 rebuild；同一仓库的 `serve` 已有
  成熟先例，会自动重建无损 legacy 来源，并对 lossy 来源告警、跳过且永不自动
  启用 `--allow-lossy-rebuild`。
- sync 已在 worker lock、fenced Store、parser registry、per-source reset 与
  marker-after-success 协议下运行，自动修复应复用这些边界，而不是进入 schema
  migration 或递归启动第二个 sync 命令。

## Requirements

- R1. 对 `rebuild=false`、`recent_days=None` 的普通 sync，在取得 worker lock、
  完成 bootstrap 且尚未发生 parser 写入时，检测本次选择范围内的 legacy
  parser-backed 来源。
- R2. 检测到可自动修复的来源时，必须先输出包含稳定来源列表的明确警告，再自动
  执行修复；这是非交互提示，不要求用户再次确认。
- R3. 自动目标只能是本次 sync 选中的 legacy parser-backed 来源。当前版本来源、
  空来源和 parserless 历史来源不得被 reset 或伪造 accounting marker。
- R4. 在任何 reset 前，对全部自动目标完成 lossy rebuild 风险预检。任一目标存在
  missing source files / protected events 时，本次普通 sync 必须在零 reset 的
  状态下失败，列出逐源风险计数与恢复/显式 opt-in 指引。
- R5. 自动路径无条件保持 `allow_lossy_rebuild=false`；即使库调用方构造了异常
  options，也不得把该字段当作普通 sync 自动接受数据丢失的授权。
- R6. 全部目标可无损时，只重置 legacy 来源；同一轮所选 parser 随后只执行一次，
  当前来源继续走增量路径，避免“先逐源 repair、再全量 sync”造成重复解析。
- R7. accounting marker 只能在对应 parser/store 流程成功后推进。解析、SQLite、
  commit、状态写入、锁丢失或取消均不得伪造 repair 完成；失败后必须可安全重试。
- R8. 带 `--recent-days` 的 bounded sync 遇到 legacy 来源时不得自动 reset。
  清空全历史后只回灌时间窗口会造成隐式历史丢失，因此应在修改前拒绝，并引导先
  运行无界 `llmusage sync` 自动修复或显式执行完整 source rebuild。
- R9. `--source <source>` 只检测和修复该来源；无 source 的普通 sync 按 parser
  registry 稳定顺序处理所选集合，并保留全部 parserless 历史与诊断状态。
- R10. CLI human stderr、`--json-events`、TUI 和 Web/JobRegistry 必须通过共享 sync
  lifecycle 表达自动修复开始/完成；JSON stdout 始终保持纯 NDJSON。
- R11. 显式 `sync --rebuild`、`--allow-lossy-rebuild` 以及现有 `serve` 启动修复
  的安全语义保持不变；重复运行在 marker 已是当前版本时为 no-op。
- R12. `source-status` / diagnostics 的建议文本与中英文 README、first-sync、
  CLI reference、安全文档及 Trellis token-accounting 合约必须同步到新行为。
- R13. 本任务不新增 schema migration，不修改 token 归一化、去重、定价或
  parserless 来源能力。

## Acceptance Criteria

- [x] AC1. fixture 中 Codex 有历史行且 marker=`2` 时，普通无界 Codex sync
  先发出自动修复提示，完整重建一次，将 marker 推进到 `3` 并成功结束。
- [x] AC2. Codex、Claude、OpenCode 同时为 legacy 且均无 lossy 风险时，一次
  普通 sync 按 registry 顺序修复三者，不要求三条人工命令，也不二次扫描来源。
- [x] AC3. legacy 与 current 来源混合时，仅 legacy 来源被 reset；current 来源
  保留增量 cursor/历史，parserless Antigravity 数据与诊断逐表保持不变。
- [x] AC4. 多个自动目标中任一个有 lossy 风险时，所有自动目标都尚未 reset；
  命令失败信息包含 source、missing file count、protected event count，历史行与
  marker 保持原状，且未启用 `--allow-lossy-rebuild`。
- [x] AC5. `sync --recent-days N` 遇到 legacy 来源时在零 reset 状态下拒绝，并
  提示先运行无界普通 sync；无 legacy 来源的 bounded sync 行为不变。
- [x] AC6. `sync --source codex` 不检查、reset 或改写其他来源；目标已经 current
  或无历史时不产生 repair lifecycle 事件。
- [x] AC7. 自动 repair 中 parser/Store 失败或取消时不发完成事件，不推进对应
  marker，并通过现有失败/取消通道返回可操作错误。
- [x] AC8. human 模式在 stderr 显示修复警告与完成边界；JSON 模式新增事件仍可
  逐行反序列化且 stdout 无普通文本；TUI/Web job 能显示合理进度文案。
- [x] AC9. 现有 serve safe/blocked/failure、显式 lossy guard、full rebuild
  parserless-preservation 回归测试继续通过。
- [x] AC10. 没有 legacy 来源时，普通 sync 的 parser、summary、run-log、事件顺序
  和可观测开销保持基线行为。
- [ ] AC11. focused token-accounting/sync-progress/job-registry tests、Rust
  format/clippy/串行全量测试、rustdoc、dashboard JS checks 与中英文 docs build
  全部通过。

## Out of Scope

- 自动设置或推断 `--allow-lossy-rebuild`。
- 在 bounded sync 中静默扩大为全历史同步，或只回灌窗口后宣称修复成功。
- 自动重建 parserless / historical-only 来源。
- 把依赖外部源文件的 rebuild 塞进 SQLite schema migration。
- 修改 token accounting 版本号、parser token 算法、报表计算或定价语义。
- 增加 GUI modal；“弹出警告”指现有 sync lifecycle 上的非阻塞 CLI/TUI/Web 提示。
