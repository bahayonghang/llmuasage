# ADR 0001 — `SourceParser` trait + `sources::registered_*` 注册表

- 状态：已采纳
- 落地阶段：阶段 3、阶段 4
- 落地日期：2026-05-06
- 相关代码：`src/parsers/source_parser.rs`、`src/parsers/driver.rs`、`src/integrations/integration.rs`、`src/sources.rs`
- 相关术语：[Source](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#1-source) / [SourceParser](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#2-sourceparser) / [Integration](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#3-integration) / [Registry](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#10-registry)

## 背景

阶段 3 / 阶段 4 之前的 `commands/sync.rs::run_once` 长这样：

```rust
sync_codex(app, store, &mut writer, parallelism).await?;
sync_claude(app, store, &mut writer, parallelism).await?;
sync_opencode(app, store, &mut writer, parallelism).await?;
```

`integrations/mod.rs::probe_all / install_all / uninstall_all` 各自硬列三连，平台分支 `cfg!(windows)` 散在 `platform_shell_command` / `platform_notify_args` 两处函数内。

加一个新源（例如 `Cursor`、`Aider`）需要改 7+ 处：枚举 + sync 三连 + integration 三连 + 平台分支可能再乘 2。

## 决策

### 1. 抽出 `SourceParser` trait（阶段 3）

`src/parsers/source_parser.rs`：
```rust
pub trait SourceParser: Send + Sync {
    fn source(&self) -> SourceKind;
    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>>;
}
```

每个源用空 ZST `pub struct CodexParser` / `ClaudeParser` / `OpencodeParser`，`impl SourceParser` 内 `Box::pin(sync_xxx(...))` 直接复用现有 async 函数体，并发结构、shard 切分逻辑零改动。原 `pub async fn sync_codex / sync_claude / sync_opencode` 降为 module 私有 `async fn`。

### 2. 抽出 `Integration` trait（阶段 4）

`src/integrations/integration.rs`：
```rust
pub trait Integration: Send + Sync {
    fn source(&self) -> SourceKind;
    fn probe(&self, app: &AppContext) -> Result<IntegrationProbe>;
    fn install(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction>;
    fn uninstall(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction>;
}
```

trait 签名为同步 `&self`。Codex/Claude/Opencode 各用 ZST + impl 委托给原模块级 `pub fn probe / install / uninstall`（保留 pub fn 是因为 `tests/local_flow.rs:177` 已直接调；trait 这层是新增的入口）。

### 3. 单点工厂 `src/sources.rs`（阶段 4）

```rust
pub fn registered_parsers() -> Vec<Box<dyn SourceParser>> { ... }
pub fn registered_integrations() -> Vec<Box<dyn Integration>> { ... }
```

`commands/sync.rs::run_once` 与 `integrations::{probe_all, install_all, uninstall_all}` 全部退化为对工厂的遍历。

### 4. 单一 driver `parsers::driver::drive`（阶段 3）

```rust
pub async fn drive(
    parsers: &[Box<dyn SourceParser>],
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    lock_wait_ms: u64,
) -> Result<Vec<SourceSyncStats>>;
```

按注册顺序串行调用每个 parser，统一注入 `lock_wait_ms`，移除 `commands/sync.rs` 里手写的 `for source in &mut sources { source.lock_wait_ms = lock_wait_ms; ... }`。

### 5. 平台分支收敛到 `HookTarget`（阶段 4）

`src/integrations/hook_target.rs::HookTarget::current(app)` 是唯一聚合 `cfg!(windows)` 的入口。`shell_command` / `notify_args` 是 match。删 `integrations/mod.rs::platform_shell_command` / `platform_notify_args`。

## 备选方案与否决理由

### 备选 A：封闭枚举 fan-out（保留现状）

```rust
match source {
    SourceKind::Codex => sync_codex(...).await?,
    SourceKind::Claude => sync_claude(...).await?,
    SourceKind::Opencode => sync_opencode(...).await?,
}
```

否决：每加一个源需要在 sync / integrations / platform 三个不同文件添加 match arm，违反"加一个源只改一处"。Deletion-test：删 trait 让 caller 不变；保留 enum-fan-out → `commands/sync.rs` 与 `integrations/mod.rs` 都要改。

### 备选 B：`async-trait` crate

否决：引入新依赖。当前 edition 2024 + tokio 1.52 下 `Pin<Box<dyn Future + Send + 'a>>` 显式包装零依赖、可读、和 `Box::pin(async_fn(...))` 调用一行写完。原生 async-fn-in-trait 的 `dyn` 兼容尚未稳，等稳定后再切换。

### 备选 C：trait 分裂为 `BatchedParser` / `StreamingParser`

否决（实施期评估后）：阶段 2 的 `commit_shard` 已把 OpenCode 流式 vs Codex/Claude 批式的差异完全压到 writer 内部。三者签名都是 `(store, writer, parallelism) -> SourceSyncStats`，trait 统一签名是自然结果，分裂会引入两套 driver。

### 备选 D：`registered_parsers()` 返回 `&'static [&'static dyn SourceParser]`

否决：当前 ZST 让静态版可行，但未来 parser 携带状态（`home: PathBuf` 用户传入根目录、按 source 配 mock 等）会被静态生命周期强制无参化。`Vec<Box<dyn Trait>>` 让升级为有字段 struct 时签名零改动。每次构造一份 ZST 拥有的 Vec，调用方按需 drop。

### 备选 E：把 `Integration` 也改成 async trait

否决：probe / install / uninstall 三动作是 fs / json / toml 同步操作，没有 await，统一同步 `&self` 最简，没有 await 损失。如果未来 integration 需要远程 probe（例如 GitHub OAuth），届时分裂为 `AsyncIntegration` 或对该方法单独包 spawn_blocking。

## Deletion-test 论证

| 删什么 | 复发现象 | 是否更深 |
|------|---------|---------|
| 删 `SourceParser` trait + driver | `commands/sync.rs` 重新硬列三连，sync_codex 等升回 pub async fn；新增第四个源时多一行 await | ✅ |
| 删 `Integration` trait + ZST | `integrations/mod.rs::install_all` 退回硬列 `codex::install / claude::install / opencode::install`；新增源时三连各加一行 | ✅ |
| 删 `src/sources.rs` registry | `commands/sync.rs` 与 `integrations/mod.rs::install_all` 都重新硬列；新增源两处 fan-out 各加一行 | ✅ |
| 删 `HookTarget` | `integrations/mod.rs` 重新写 `cfg!(windows) { ... } else { ... }` 两个 fn；codex/claude/opencode 重新 import；第三个平台分支会让两个函数各加一个 if | ✅ |

## 后果

- "新增第四个源"现在只改两处：`SourceKind` 枚举 + `sources.rs` 工厂的两个 vec。Driver / fan-out / 平台分支自动跟随。
- `Box<dyn SourceParser>` / `Box<dyn Integration>` 引入 vtable 间接 + 一次堆分配（ZST 不分配数据，只分配 vtable 指针），单次 sync 可忽略；driver 串行迭代不构成热点。
- `parse` 用 `Pin<Box<dyn Future>>` 写起来比 `async fn` in trait 略繁琐，但 `Box::pin(sync_xxx(...))` 一行包装可读性可接受，且不引依赖。
- `tests/local_flow.rs` 现在依赖 `integrations::HookTarget::current(...).shell_command(...)` 入口；后续若要 mock 平台，可以扩 `HookKind` 不破坏调用方。

## 验证

- 阶段 3 完成时：`rtk cargo build` / `cargo fmt --check` / `clippy -D warnings` / `cargo test --test-threads=1` 全绿（35/35 测试）。
- 阶段 4 完成时：同上（35/35 测试）；`tests/local_flow.rs::init_writes_quoted_windows_string_commands_for_spaced_paths` 走新 `HookTarget` 路径无回归。
