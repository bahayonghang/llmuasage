# sync 进度条生命周期执行计划

## Checklist

1. **依赖**：Cargo.toml 加 `indicatif`；`cargo build`；确认 Cargo.lock 传递依赖仅 indicatif 树。
2. **纯重构（独立 commit，回滚点）**：`HumanProgress` 迁为 `LineRenderer`，抽象 `HumanRenderer` 入口，行为零变化。验证：`cargo test --lib commands::sync -- --test-threads=1` + `cargo test --test m2_raw_archive_logs -- --test-threads=1`。
3. **生命周期重构**：`run_with_human_events` 入口建 renderer + `TerminalGuard`；reporter 与 guard 的所有权按 design §1 落地；提前返回路径全被 guard 覆盖。
4. **BarRenderer**：阶段映射 + 分来源单位（design §2）+ `stderr_with_hz(10)` 节流。
5. **Ctrl-C**：human 与 JSON 路径接 token；`run_once_with_options` 调用点逐一核实（web/TUI 不受影响）。
6. **测试**：注入 hidden draw target 的全序列/失败/取消/bootstrap 错误测试；非 TTY 无 ANSI 子进程测试；`LLMUSAGE_PROGRESS=off` 行为测试。
7. **手动冒烟**（PowerShell）：`cargo run -- sync`；触发一次 pricing 阶段（rebuild）；一次 Ctrl-C；`cargo run -- sync 2>$null` 确认 stdout 纯净；`$env:LLMUSAGE_PROGRESS='off'` 对照。
8. 修正 `Progress` 变体注释（parsers/mod.rs:79-81）。
9. `cargo fmt --all -- --check`、严格 Clippy、`cargo test -- --test-threads=1`。

## 风险点 / 回滚点

- 步骤 2 独立 commit；步骤 3-5 一个 commit；任一不过审回滚到步骤 2。
- 回归哨兵：sync.rs:733-792 pricing 文案单测、m2_raw_archive_logs NDJSON 断言、sync_regression 取消语义测试，每步必跑。
