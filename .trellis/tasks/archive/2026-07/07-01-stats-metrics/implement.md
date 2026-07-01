# 执行计划 — Token 统计口径增强（Child A）

> 本轮只出规划；以下为待批准后的执行顺序。每步含验证与回滚点。

## 前置确认（gate 0）
- [ ] 读 `.trellis/spec/llmusage/backend/source-sync-contracts.md`
- [ ] 核对 `src/commands/session.rs` + `src/query/reports.rs` 是否已含 session 级
      active/span → 决定 A3 是否保留 / 标记 N/A
- [ ] 抽查 `src/parsers/claude.rs` 单事件是否可得"最近请求 input+cache tokens"

## 步骤 1 — A5 longest streak（最小、先落）
- [ ] `src/tui/panels/stats.rs`：加 `longest_streak()`，Health 行改 `current/longest`
- [ ] 单测：全零 / 单段 / 多段 / 末尾连续
- 验证：`cargo test stats` + 目视 `llmusage tui` Health 面板
- 回滚：删函数 + 还原展示行

## 步骤 2 — A1 model context-window 映射
- [ ] `pricing/static-v1.json` + `src/query/pricing_catalog.rs`：补 `context_window`
- [ ] `model_context_window(model) -> Option<u32>`，复用现有匹配逻辑
- [ ] 单测：命中 / prefix 命中 / 未命中降级 None
- 验证：`cargo test pricing_catalog`
- 回滚：移除新增字段与函数（catalog 向后兼容）

## 步骤 3 — A1 context_pressure 查询 + 展示
- [ ] `src/query/mod.rs`：`context_pressure(filter) -> ContextPressurePayload`
- [ ] Overview 或 Health 面板加"ctx peak/avg %"一行；无样本显示 `-`
- [ ] 单测：混合可算/不可算样本，unpriced 不入分母
- 验证：`cargo test context_pressure` + TUI 目视
- 回滚：移除 query 方法与展示行

## 步骤 4 —（条件）A3 session active/span
- [ ] 仅当 gate 0 判定未覆盖：`query` 加 gap-capped active-minutes 纯函数 + 输出
- [ ] session 报告展示 `active/span`
- [ ] 单测：含 >30min gap、连续、单事件
- 回滚：独立移除

## 收尾（gate final）
- [ ] `cargo fmt --check` && `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test -- --test-threads=1`
- [ ] 更新 `.trellis/spec/llmusage/backend/` 相关契约（若引入 context_window catalog 键）
- [ ] `task.py finish` → parent 汇总

## 审查门
- 步骤 2 完成后暂停：确认 catalog schema 变更方向获认可，再做步骤 3。
