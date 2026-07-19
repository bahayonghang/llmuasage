# Serve 自动修复 token accounting 合约迁移

## Goal

让从旧 token accounting 合约升级的用户在启动 `llmusage serve` 时自动完成可安全重建的 parser-backed 源迁移，从而解除后续普通 `llmusage sync` 的写入保护；同时保证所有 rebuild 删除范围都严格限制在真正可由 parser 重建的来源内。

## Background

- 2026-07-16 的 token accounting 对齐引入了每源 `token_accounting_version=2` 标记。存在历史行但没有当前标记的源会被视为 legacy；普通写入在 `src/commands/sync.rs:492` 被拒绝。
- 当前 `serve` 只执行 store bootstrap 后启动 Web 服务，没有处理 legacy accounting（`src/commands/serve.rs:20`）。
- 用户当前数据库中的 `codex`、`claude`、`opencode` 均为 legacy；三者的 `lossy_rebuild_risk` 均为 `false`。
- 用户当前 `antigravity` 有 43 条受保护历史记录和 5 个缺失文件，但它没有 parser capability，也不是 token accounting legacy 修复目标。
- 无 source 的 `sync --rebuild` 当前调用 `Store::reset_usage_data()` 删除所有来源，但 lossy guard 和后续 parser fan-out 只遍历 `registered_parsers()`。因此 parserless Antigravity 历史可能在未被预检、也无法重建的情况下被删除。
- 自动修复必须逐个选择 parser-backed legacy 源，不能执行无 source 的全源 rebuild。
- Web JobRegistry 已调用 `commands::sync::run_once_with_cancel`，不会绕过同一 token accounting 写入保护。

## Requirements

- R1. `llmusage serve` 在绑定端口和打开浏览器前检测所有 parser-backed 源的 token accounting 状态。
- R2. 对每个 legacy parser-backed 源，复用现有 `sync --rebuild --source <source>` 行为逐源重建。
- R3. 自动路径始终保持 `allow_lossy_rebuild=false`，不得自动接受不可重建历史丢失。
- R4. parserless、无历史行、或已经处于当前 accounting 版本的源不得被重建。
- R5. 每个成功重建的源仅在 parser/store 同步成功后写入当前版本标记；失败不得伪造迁移成功。
- R6. 没有 legacy 源时，`serve` 启动行为和开销保持基本不变。
- R7. 保留普通 `sync` 对 legacy 写入的拒绝，避免其他入口静默混合两套口径。
- R8. CLI 输出和运行日志应能说明自动重建了哪些源，失败信息应包含现有安全修复指引。
- R9. 已知 lossy 风险只跳过对应源并输出警告，`serve` 继续提供历史报表；该源的普通写入仍由现有 guard 拒绝。
- R10. 已通过 lossy 预检的自动重建若发生解析、SQLite 或提交异常，`serve` 必须启动失败，不得把可能发生在 reset 后的异常降级为警告。
- R11. 无 source 的 `sync --rebuild` 只能重置 `registered_parsers()` 中的来源；parserless 来源的 event、bucket、行为事实、cursor 和 source-file 状态必须保留。
- R12. 全量 rebuild 的 lossy 预检集合、实际 reset 集合和 parser fan-out 集合必须来自同一个 parser registry，避免未来新增来源时再次漂移。

## Acceptance Criteria

- [x] AC1. fixture 中存在 Codex 历史行但缺少 accounting 标记时，`serve` 启动准备会自动逐源重建并把 Codex 标记推进到版本 2。
- [x] AC2. 同一 fixture 完成 `serve` 启动准备后，普通 Codex sync 不再触发 legacy/current 混用错误。
- [x] AC3. 多个 legacy parser-backed 源会按稳定顺序逐个重建，不执行全源 reset。
- [x] AC4. 某 legacy 源存在 missing source files 时，自动重建不会启用 `--allow-lossy-rebuild`，旧历史不会被删除。
- [x] AC5. parserless Antigravity 即使有 missing files，也不会阻止无风险的 Codex/Claude/OpenCode 逐源修复。
- [x] AC6. 当前版本源和空源不会被重建；重复启动 `serve` 幂等。
- [x] AC7. legacy parser-backed 源存在 lossy 风险时，`serve` 仍能启动看板，但该源保持 legacy 且历史行不被删除。
- [x] AC8. 安全源重建发生非 lossy 异常时，`serve` 不绑定端口并返回错误。
- [x] AC9. 无 source 的 full rebuild 会重建 parser-backed 来源，但保留 parserless Antigravity 的全部历史与诊断状态。
- [x] AC10. full rebuild 的任一 parser-backed 来源有 missing files 时仍由现有 lossy guard 拒绝，并且 reset 尚未发生。
- [x] AC11. 现有 `legacy_source_requires_guarded_explicit_rebuild_before_new_writes` 合约测试继续通过。
- [x] AC12. focused tests、`cargo fmt --check`、Clippy、串行全量 Rust tests 和 docs build 通过。

## Out of Scope

- 自动启用 `--allow-lossy-rebuild`。
- 改变 token 归一化算法或 `TOKEN_ACCOUNTING_VERSION`。
- 自动重建 parserless Antigravity 历史。
- 将普通 `sync` 改成无条件自动迁移。
