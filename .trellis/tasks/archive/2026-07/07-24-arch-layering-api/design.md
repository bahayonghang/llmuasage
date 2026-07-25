# Design: 应用分层与 1.x public API 收口

## 问题现状

| 缺陷      | 位置                                             | 影响                                                       |
| --------- | ------------------------------------------------ | ---------------------------------------------------------- |
| ARCH-002  | `sync::JobRegistry` 直接 import `commands::sync` | domain 层反向依赖 CLI adapter，重构 commands 就必须改 sync |
| MAINT-001 | `query/mod.rs` 5,045 行, `web/mod.rs` 4,991 行   | 可读性差，合并冲突频繁，测试边界模糊                       |
| API-002   | `lib.rs` pub-export 所有实现模块                 | 1.x 下形成意外 semver 承诺，阻碍内部重构                   |

## 分层目标

```
┌──────────────────────────────────────────────────┐
│  Adapters  │  commands/  │  web/  │  tui/         │
│            └─────────────┴────────┴───────────── │
│                          ↓ only                   │
│  Application │  sync/SyncService  │  query layer  │
│              └────────────────────────────────── │
│                          ↓ only                   │
│  Domain     │  store/  │  parsers/ │  models/     │
└──────────────────────────────────────────────────┘
```

## 实施顺序（本次 session）

### Phase 1: ARCH-002 — 断开 sync→commands 反向依赖 ✅ planned

1. 将 `SyncRunOptions` 和 `SyncSummary` 从 `commands/sync.rs` 移至
   `src/sync/types.rs`（它们是 sync-domain 类型，不是 CLI-adapter 类型）。
2. `commands/sync.rs` 重新导出 `pub use crate::sync::types::*` 保持 API 兼容。
3. 在 `src/sync/executor.rs` 定义 `SyncExecutor` trait：
   ```rust
   #[async_trait]
   pub trait SyncExecutor: Send + Sync {
       async fn run(&self, req: SyncRequest) -> Result<SyncSummary>;
   }
   ```
4. `JobRegistry` 持有 `Arc<dyn SyncExecutor>`，`run_job` 通过 trait 调用。
5. `commands::sync` 实现 `SyncExecutor`（唯一适配器），CLI/Web 注入时构造。
6. CI 检查：`sync` 模块不得 import `commands::*`
   （`grep "use crate::commands" src/sync/` 须为空）。

### Phase 2: API-002 — CI 门控 ✅ planned

- `cargo-semver-checks` 进 CI（对 `main` 分支 diff）。
- `unstable` feature flag + 文档注记。
- 模块大小门控：当前尺寸记录为基线，CI 报告超标文件（暂不硬失败）。

### Phase 3: MAINT-001 — God module 拆分 📋 后续

- `web/mod.rs` → `web/{router, guard, state, cache, dto}.rs`
- `query/mod.rs` → `query/{dashboard, trend, ranking, activity, compare}.rs`
- 目标：单文件 <800 行；达标后加 CI 硬失败门控。
- 预计工作量：2-4 周独立 PR，本次不实施。

### Phase 4: Public API 收口 📋 后续

- 实现模块改 `pub(crate)`；`lib.rs` 只导出 façade。
- 加 `cargo-semver-checks` snapshot。
- 发布 CHANGELOG deprecation 计划。
