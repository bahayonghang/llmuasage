# 分组提交方案

状态：用户于 2026-09-28 确认本方案；两个工作提交均已完成，分别为 `cc7eed3` 和 `6389ceb`。三个子任务已归档，父任务归档状态由 task.json 记录，最终 journal 记录完整交付结果。

范围：本轮已识别的 67 个产品、文档和规范文件。使用本地 Git，保留当前 `dev` 分支，不执行 push、tag、发布或安装。

同步诊断、预检、writer 测量和版本准备共享多个源码、规范与双语文档文件，因此归入同一工作提交，按文件边界暂存。任务材料由后续归档流程保存。

## 1. fix(测试): 🐛 校正时区回归与桌面格式门禁

Why: 原回归测试依赖固定本地偏移或隐式宿主时区，导致三个门禁失败；桌面测试存在格式差异。

文件数：4。

- `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`
- `desktop/src-tauri/tests/quota.rs`
- `src/query/filter.rs`
- `src/web/mod.rs`

## 2. feat(同步)!: ✨ 完善诊断与重放写入并准备 2.0 版本

Why: 来源故障需要独立诊断和历史保护；受阻重放与路径 reset 存在重复工作。现有公开 API 差异使用已批准的 2.0.0 版本边界。

文件数：63。

- `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md`
- `.trellis/spec/llmusage/backend/runtime-log-contracts.md`
- `.trellis/spec/llmusage/backend/source-sync-contracts.md`
- `.trellis/spec/llmusage/backend/token-accounting-contracts.md`
- `.trellis/spec/llmusage/backend/tui-subscription-contracts.md`
- `.trellis/spec/llmusage/backend/write-fencing-contracts.md`
- `CHANGELOG.md`
- `Cargo.lock`
- `Cargo.toml`
- `README.md`
- `README.zh-CN.md`
- `desktop/package-lock.json`
- `desktop/package.json`
- `desktop/src-tauri/Cargo.lock`
- `desktop/src-tauri/Cargo.toml`
- `desktop/src-tauri/tauri.conf.json`
- `docs/.vitepress/config.ts`
- `docs/dashboard/index.md`
- `docs/guide/first-report.md`
- `docs/guide/getting-started.md`
- `docs/index.md`
- `docs/reference/cli.md`
- `docs/reference/library-api.md`
- `docs/reference/migration-v2.md`
- `docs/zh/dashboard/index.md`
- `docs/zh/guide/first-report.md`
- `docs/zh/guide/getting-started.md`
- `docs/zh/index.md`
- `docs/zh/reference/cli.md`
- `docs/zh/reference/library-api.md`
- `docs/zh/reference/migration-v2.md`
- `src/commands/diagnostics.rs`
- `src/commands/doctor.rs`
- `src/commands/source_status.rs`
- `src/commands/sync.rs`
- `src/commands/sync_progress.rs`
- `src/commands/sync_summary.rs`
- `src/domain/mod.rs`
- `src/domain/models.rs`
- `src/domain/source_diagnostics.rs`
- `src/parsers/antigravity.rs`
- `src/parsers/antigravity/decode.rs`
- `src/parsers/antigravity/tests.rs`
- `src/parsers/codex.rs`
- `src/parsers/driver.rs`
- `src/parsers/file_state.rs`
- `src/parsers/grok.rs`
- `src/parsers/mod.rs`
- `src/parsers/source_files.rs`
- `src/parsers/writer_benchmark.rs`
- `src/query/diagnostics.rs`
- `src/remote/importer.rs`
- `src/remote/protocol.rs`
- `src/runtime/logging.rs`
- `src/store/mod.rs`
- `src/store/sync_status.rs`
- `src/store/sync_writer.rs`
- `src/store/sync_writer/profiling.rs`
- `src/store/sync_writer/profiling/tests.rs`
- `src/sync/engine.rs`
- `tests/sync/runtime/progress_io.rs`
- `tests/sync/sources/antigravity.rs`
- `tests/sync/sources/zcode.rs`

## 任务材料与收尾

两个工作提交成功后，先按实施顺序归档三个子任务，再归档父任务，最后记录开发日志。保留诊断、失败候选、正式基准、门禁结果和源码身份材料。归档和日志提交由 Trellis 脚本生成，不与工作提交交错。

- `.trellis/tasks/09-26-sync-diagnostics-contracts/`
- `.trellis/tasks/09-26-antigravity-replay-preflight/`
- `.trellis/tasks/09-26-sync-write-profiling/`
- `.trellis/tasks/09-26-sync-diagnostics-performance/`

## 未识别改动

当前没有已发现的无关脏文件。最终提交前重新核对文件清单；新增或范围外文件不自动纳入。所有候选文件均小于 1 MiB，未发现数据库、可执行文件或常见密钥文件后缀。

## 确认依据

`.trellis/workflow.md` Phase 3.4 要求：`Present the plan once, ask for one-shot confirmation`。确认对象为本文件的两个工作分组以及后续四个任务归档和日志记录。

## 已执行的工作提交

- `cc7eed395b56d3c9ee524aa69cc8a3a1778046e0`：fix(测试): 🐛 校正时区回归与桌面格式门禁
- `6389cebb9a5f60e624da3122eb3ac18371e450eb`：feat(同步)!: ✨ 完善诊断与重放写入并准备 2.0 版本

## 已完成的子任务归档

- `82929bb32a39ec25c4de51d837923c91b8b8e190`：09-26-sync-diagnostics-contracts
- `ef62682ce0971805d9725e26a39ac74c682c360c`：09-26-antigravity-replay-preflight
- `09e2944a28d76ca0093ba77d5796bb2addfcda39`：09-26-sync-write-profiling

归档脚本对未跟踪旧目录的 pathspec 自动提交失败已限定范围处理，见 `research/archive-recovery.md`。
