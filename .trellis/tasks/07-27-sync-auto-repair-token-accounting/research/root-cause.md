# Root Cause And Safety Findings

## Reproduction Interpretation

用户日志顺序是：

1. schema v17 `add_source_sync_parse_issues` 完成；
2. 普通 `sync` 进入 source pipeline；
3. token accounting guard 报
   `Refusing to mix legacy and current token accounting`。

因此 v17 migration 已成功，报错来自独立的 accounting version guard，不应通过
新增/重跑 schema migration 修复。

## Confirmed Code Facts

1. `src/commands/sync.rs:466-479` 先按 `--source` 选 parser，再调用
   `assert_token_accounting_write_allowed`。
2. `src/commands/sync.rs:666-685` 对非 rebuild 请求发现 legacy 后无条件报错，要求
   用户逐源执行显式 rebuild。
3. `src/commands/sync.rs:688-723` 已有 missing-files/protected-events lossy guard；
   显式 `allow_lossy_rebuild` 会绕过它，因此自动 policy 不能直接复用该授权分支。
4. `src/commands/sync.rs:583-598` 在 parser writer 完成后才推进 per-source marker
   并写 sync status，提供了正确的成功边界。
5. `src/commands/serve.rs:206-263` 已实现安全自动 repair 先例：legacy 发现、逐源
   risk 查询、safe rebuild、blocked warning，且显式固定
   `allow_lossy_rebuild=false`。
6. `.trellis/spec/llmusage/backend/token-accounting-contracts.md:15-23` 记录当前
   Codex marker=`3`，Claude/OpenCode/Kimi/Pi/Grok marker=`2`；`:68-80` 记录 serve
   repair 与 parserless-preserving full rebuild 合约。
7. `tests/token_accounting_parity.rs:130-195` 当前把“普通 sync 必须失败、显式 rebuild
   后恢复”锁成回归；本任务需要有意替换这一产品合约，而不是绕过测试。
8. `tests/token_accounting_parity.rs:199-350` 已覆盖 serve safe/multi-source/blocked/
   parser-failure fixture，可复用数据搭建和风险断言。

## Root Cause

token accounting 版本化正确阻止了新旧口径混写，但普通 sync 的升级编排仍停留在
“报错 + 让用户手工枚举来源”。同一安全 rebuild 能力已经存在，缺的是在普通 sync
共享 pipeline 内把检测、无损预检、targeted reset、反馈和 marker 成功边界组合成
一个原子用户流程。

## Important Edge Case: Bounded Sync

`ValidatedSyncRequest` 允许 `recent_days`，driver 对本轮所有 parser 使用同一个
`recent_cutoff`。如果普通 bounded sync 自动 reset legacy source，然后继续沿用
cutoff，窗口外历史会被删除却无法回灌，且现有 post-driver 逻辑仍可能推进 marker。

所以 MVP 必须在 `legacy + recent_days` 时零 reset 拒绝。支持“一部分 source 全量
repair、另一部分 source bounded sync”需要 per-source cutoff 或两阶段 driver，
不属于本问题的最小完整修复。

## Recommended Policy

- 普通无界 sync：全部 selected legacy sources 先做 lossless preflight；全部安全
  才在同一 fenced run 内 reset legacy subset 并让 selected parsers 只跑一次。
- 任一 selected legacy source blocked：本次 sync 在任何 reset 前失败，保留所有
  历史和 marker。
- serve：继续现有 best-effort 逐源 repair/skip 策略，因为它的目标是尽量启动
  read-only dashboard，和普通 sync 的完成语义不同。
- bounded sync：引导先运行无界普通 sync 自动 repair。
- parserless source、schema migration、token algorithms 均不进入本任务。
