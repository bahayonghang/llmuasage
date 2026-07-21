# sync 摘要展示执行计划

轻量任务，设计即 prd.md 的 R1-R6；无独立 design.md。

## Checklist

1. Cargo.toml 加 `console`（版本与 indicatif 依赖树对齐）；`cargo build`。
2. 实现 `format_summary_lines` + 字节/耗时格式化小函数（src/commands/sync.rs 内或新 `sync_summary.rs` 模块）。
3. 改造 `print_summary` 调用纯函数，`color = stdout().is_terminal()`。
4. 单测：对齐、color=false、absent、last_error、零值/大数格式化。
5. 手动：TTY 看着色；`cargo run -- sync | Out-String` 确认无 ANSI。
6. `cargo fmt --all -- --check`、严格 Clippy、`cargo test -- --test-threads=1`。

## 风险点

- 列宽计算需按去掉 ANSI 后的显示宽度：实现时颜色在拼接对齐之后套用，或先算宽后着色（推荐后者，单测直证）。
