# TUI 紧凑数字显示实施计划

## 实施顺序

- [x] 确认后缀使用大写 `K/M/B/T`，并对 `prd.md` 做最终 convergence pass。
- [x] 在 `src/tui/format.rs` 先补失败的表驱动测试，固定 K/M/B/T 阈值、精度、进位、负值与
  `i64::MIN` 行为。
- [x] 实现单一交互 TUI `stat_compact` helper，不改变 `grouped`、`token_compact`、cost、percent
  等既有输出契约。
- [x] 先迁移 Overview 与 Footer，并新增截图量级的宽/窄 `TestBackend` 回归测试。
- [x] 按 design 调用点矩阵迁移 Models、Daily、Hourly、Cost、Stats、Behavior、Blocks；明确保留
  Usage 同步诊断计数的精确显示。
- [x] 更新 `tests/tui_panels_prop.rs`，以生产 formatter 构造预期值并保留排序、windowing、selection
  行为断言。
- [x] 评估迁移后的旧 helper；`tokens` / `footer_compact` 属于既有公开命名契约，予以保留，
  不清理任务外既有代码。
- [x] 更新 `.trellis/spec/llmusage/backend/tui-presentation-contracts.md` 的签名、规则和测试矩阵。
- [x] 检查最终 diff，确认未触及 query/store/parser/web/CLI 输出逻辑。

## 验证命令

```powershell
cargo test tui::format --lib
cargo test --test tui_panels_prop -- --test-threads=1
cargo test tui::report_table --lib
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --test-threads=1
git diff --check
```

## Review Gates

- 任务在用户审核规划前保持 `planning`，不得运行 `task.py start`。
- formatter 边界测试先失败、实现后通过。
- 逐项核对 R2 调用点，不能以全局文本替换代替字段语义分类。
- CLI report table 与 Usage 同步诊断计数的精确输出是回归红线。

## Rollback Point

改动没有数据迁移。若渲染回归或可读性不符合预期，整体回滚 formatter、调用点、测试和契约文档
这一原子提交即可；不得只回滚部分面板而留下混合格式。
