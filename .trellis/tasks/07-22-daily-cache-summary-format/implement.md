# Daily cache 汇总展示修复 - 实施计划

## Implementation

- [x] 在 `src/tui/report_table.rs` 添加统一的表格可见总量 helper，使用饱和加法。
- [x] unified/focused 的普通行、Agent 子行、model breakdown 和最终 Total 行改用四个
      可见分量之和；不改 DTO/JSON 字段。
- [x] 上述表格的 token 单元格统一复用 `format_token_compact`，覆盖 full/compact 与
      `--no-cost` 组合。
- [x] 通用 renderer 在 Total 行前绘制双线分隔，保持其他行和列宽逻辑不变。
- [x] 更新 `src/tui/report_table.rs` 单元测试：隐藏 reasoning/extra 不进入表格总量、
      K/M/B 格式、无原始大数、Total 双线分隔。
- [x] 更新 `tests/report_commands.rs` 文本集成断言，并保留 JSON 权威总量断言。

## Validation

```powershell
cargo test tui::report_table --lib
cargo test --test report_commands -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
```

若默认 `target` 被运行中的 llmusage 进程锁定，使用独立的临时
`CARGO_TARGET_DIR`，不终止用户进程。

## Review Gates

- 文本总量测试必须使用一个 `total_tokens` 大于四个可见分量之和的 fixture，证明修复
  不是对相等样本的空断言。
- JSON 测试必须继续断言原始 `totalTokens`，防止 presentation helper 泄漏到数据层。
- 最终 diff 不包含 `Cargo.toml`/`Cargo.lock` 的用户版本改动或无关格式化。

## Codex Replay Follow-up

- [x] 在 `src/parsers/codex.rs` 移植 ccusage 的 replay marker + 同秒前缀检测，跳过父历史
      并保留 cumulative baseline。
- [x] 添加 thread-spawn/fork replay、普通同秒事件和 baseline 的 parser 回归测试。
- [x] token accounting 改为 source-aware expected version：Codex `3`，其他 parser
      source `2`；更新 sync status 与 marker 测试。
- [x] 在隔离 `LLMUSAGE_HOME` 上重放真实 Codex 文件，对照 ccusage 两日 JSON。
- [x] 运行 `trellis-check` 要求的聚焦测试、串行全测试、Clippy 和 `just ci`。
