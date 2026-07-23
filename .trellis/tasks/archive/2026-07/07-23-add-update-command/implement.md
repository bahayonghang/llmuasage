# 自更新命令实施计划

## Implementation Checklist

- [x] 在 `src/commands/update.rs` 定义仓库/包名常量、更新渠道、安装参数、
  确认状态机、可注入执行边界和生产 Cargo runner。
- [x] 在 `src/commands/mod.rs` 注册模块、`Update` 参数和 dispatch 分支；保证
  默认 `main`、显式 `dev`、`--check` 与非法渠道帮助/错误契约。
- [x] 添加聚焦单元测试，覆盖参数构造、确认/取消/重试/EOF、check-only
  零执行、成功与启动/退出失败传播。
- [x] 更新 `README.md`、`README.zh-CN.md`、英文/中文安装指南。
- [x] 将稳定的自更新命令契约补入 `.trellis/spec/llmusage/backend/` 并更新索引。
- [x] 检查最终差异只包含任务代码、测试、文档、spec 与任务文件。

## Validation

按由小到大的顺序运行：

```powershell
cargo test commands::update --all-features -- --test-threads=1
cargo test commands::tests::update --all-features -- --test-threads=1
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
npm --prefix docs run docs:build
just ci
git diff --check
```

另运行只读 CLI smoke：

```powershell
cargo run -- update --help
cargo run -- update --check
cargo run -- update dev --check
```

不得在验证中运行实际 `cargo run -- update`，避免覆盖当前开发机安装。

## Review Gates

- `--check` 和拒绝确认路径没有进程执行记录。
- 安装参数没有 shell 拼接，仓库和渠道不能被用户替换为任意值。
- 错误保留退出码/启动原因和完整人工复现命令。
- 更新逻辑不打开 Store、不执行 init/sync/uninstall、不修改用户数据。
- 中英文文档命令与 Clap 帮助一致，`dev` 风险可见。

## Rollback Points

- 若 Clap 契约不稳定，先回退命令枚举/dispatch 与 `update.rs`，不保留半成品文档。
- 若 Cargo runner 在受支持平台失败，任务保持未完成并记录平台证据；不要以
  Release 下载器或新生产依赖扩大范围。

## Verification Evidence

- 2026-07-23: 7 个 `commands::update` 测试与 2 个 Clap update 测试通过。
- 2026-07-23: `cargo run -- update --help`、`update --check` 和
  `update dev --check` smoke 通过，未执行真实安装。
- 2026-07-23: `cargo clippy --all-targets --all-features -- -D warnings` 通过。
- 2026-07-23: VitePress 双语文档构建通过。
- 2026-07-23: `just ci` 通过，包括 442 个 Rust 单元测试、全部集成测试、
  rustdoc、Dashboard JavaScript 检查和 VitePress 构建；3 个测量测试按设计 ignored。
- 2026-07-23: `git diff --check` 与 Trellis context validation 通过。
