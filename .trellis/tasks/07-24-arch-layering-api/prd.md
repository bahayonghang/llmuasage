# 应用分层与 1.x public API 收口（ARCH-002、MAINT-001、API-002）

## Goal

引入 application service 分层消除业务层对 CLI 层的反向依赖，拆分 God module，并收紧 1.x public API 到稳定 façade。这是审计明确要求**最后执行**的长期架构任务。

## 覆盖发现（已核实）

- **ARCH-002（P2）**：`src/sync/job_registry.rs:15-20,329-382` `sync::JobRegistry` 直接 import/call `commands::sync::{SyncRunOptions, run_once_with_cancel}`——application/domain 层反向依赖 CLI adapter，Web job orchestration 被 CLI command API 绑死。
- **MAINT-001（P2）**：实测行数：`src/query/mod.rs` 5,045 行、`src/web/mod.rs` 4,799 行、`src/query/explorer.rs` 2,111 行、`src/store/sync_writer.rs` 1,969 行。单文件混合 router、security、cache、DTO、query execution、lifecycle、tests。
- **API-002（P2，设计债）**：`Cargo.toml` 版本 1.0.2，但 `src/lib.rs:3-39` 公开 `commands/common/domain/parsers/registry/runtime/tui/web` 等实现模块，注释自称"0.7.x compatibility、可能变化"——1.x 下这些 public item 默认形成 semver 承诺，阻碍内部重构。

## Requirements

1. 提取 `application::SyncService` 与 typed `SyncRequest/SyncResult`；JobRegistry 依赖 trait/service；CLI/Web/TUI 全部只做 adapter（迁移顺序见审计 §3.3.3：先抽 service 不改行为 → registry 换依赖 → adapter 接入 → store 拆 repository → 最后收 pub）。
2. God module 按 vertical slice 拆分：web 拆 route/guard/state/query/cache；store 拆 transaction protocol/repository；设定单模块 <800 行预算并加 CI 检查。
3. public API 收口：公开 `AppPaths`、Store 安全 façade（或 repository trait）、`Dashboard/QueryFilter`、typed SyncRequest/SyncResult、source descriptors；实现模块改 `pub(crate)`；两个 release 周期 deprecate；CI 加 `cargo-semver-checks` 与 public API snapshot；`unstable` feature 明确不保证 semver。
4. 强制依赖规则（审计 §2.4）：adapters→application→domain；commands/web/tui 互不 import；mutation API 要求 WritePermit 类型。

## Acceptance Criteria

- [ ] `sync` 模块不再 import `commands::*`（编译期依赖检查/lint）。
- [ ] 四个热点文件全部 <800 行（tests 可独立），CI 脚本 gate。
- [ ] `cargo-semver-checks` 进 CI；public API snapshot 建立；deprecation 计划写入 CHANGELOG/docs。
- [ ] 全量行为回归：`just ci` 通过，CLI/Web/TUI 输出与重构前一致。

## Notes

- **顺序约束**：必须在 correctness 类子任务（sec-public-boundary、jsonl-partial-tail、write-fencing-coordinator、pricing-atomicity）稳定后执行，避免在未闭合的正确性边界上制造巨大 diff（审计 §5 第 6 条）。
- 预计 3-8 周，建议再拆二级子任务（可用 task.py 在本任务下继续 create --parent）。
- 复杂任务：启动前必须补 design.md + implement.md。
