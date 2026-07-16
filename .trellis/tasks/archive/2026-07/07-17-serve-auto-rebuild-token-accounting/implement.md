# Implementation Plan

## 1. Regression Tests First

- [x] 在 `tests/token_accounting_parity.rs` 增加 serve repair 测试：seed Codex、完成首次 sync、清 marker、调用 repair，断言 marker=2 且普通 sync 成功。
- [x] 增加多个 legacy parser source 的稳定逐源修复测试，断言没有触碰 parserless Antigravity。
- [x] 增加 lossy legacy source 测试，断言 repair 报告 blocked、历史行未删除、marker 仍为 legacy，且函数整体成功。
- [x] 增加 full rebuild 回归测试：seed parser-backed 与 parserless 历史后执行无 source rebuild，断言 Antigravity 数据和状态完整保留。
- [x] 先运行 focused test 并确认新行为测试在实现前失败。

## 2. Centralize Legacy Detection

- [x] 在 `src/commands/sync.rs` 提取 `legacy_token_accounting_sources`，使用 parser registry 作为唯一 fan-out。
- [x] 让现有普通 sync guard 复用该函数，并保留 source-filter 行为。
- [x] 让无 source rebuild 按 parser registry 逐源 reset，移除 command path 对无条件 `reset_usage_data()` 的调用。
- [x] 保持 `assert_lossless_rebuild` 和 `allow_lossy_rebuild` 语义不变。

## 3. Add Serve Startup Repair

- [x] 在 `src/commands/serve.rs` 定义 repair report/blocked source 数据结构。
- [x] 实现逐源 lossy 预检和 `sync::run_with_options` 调用；所有自动调用显式保持 `allow_lossy_rebuild=false`。
- [x] 将 repair 放在 bootstrap 之后、`web::serve` 之前。
- [x] 为 repaired、blocked 和 unexpected failure 添加结构化日志与清晰终端文本。

## 4. Documentation

- [x] 更新 `README.md` 与 `README.zh-CN.md` 的 legacy accounting 安全说明。
- [x] 更新 `docs/guide/first-sync.md` 与 `docs/zh/guide/first-sync.md`。
- [x] 更新 `docs/dashboard/index.md` 与 `docs/zh/dashboard/index.md` 的 serve 启动行为。
- [x] 更新 `docs/safety/index.md` 与 `docs/zh/safety/index.md` 的 full rebuild 范围。

## 5. Validation

- [x] `cargo test --test token_accounting_parity -- --test-threads=1`
- [x] `cargo test commands::serve:: -- --test-threads=1`（由全量 Rust tests 覆盖）
- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test -- --test-threads=1`
- [x] `npm --prefix docs run docs:build`
- [x] 在隔离 fixture 中复跑 serve-startup repair 反馈环路。
- [x] 对真实数据库只做 `source-status`/diagnostics 验证；除非用户另行要求，不在开发测试期间执行真实 rebuild。

## Risk And Rollback Points

- 修改 reset/guard 参数前先确认 `allow_lossy_rebuild` 始终为 false。
- repair 必须发生在端口绑定前，避免 UI 在迁移中读取部分状态。
- 不修改 Store bootstrap/schema migration，避免所有只读命令意外触发外部文件扫描。
- 如果 focused regression 无法稳定覆盖真实启动 seam，停止实现并调整 seam，不用浅层 mock 替代。
