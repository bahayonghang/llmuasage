# sync 进度条生命周期设计

## 1. 模块结构

新模块 `src/commands/sync_progress.rs`：

```rust
pub enum HumanRenderer {
    Line(LineRenderer),   // 非 TTY / LLMUSAGE_PROGRESS=off
    Bar(BarRenderer),     // TTY indicatif
}
impl HumanRenderer {
    pub fn new(draw: ProgressDrawTarget, force_line: bool) -> Self;
    pub fn render(&mut self, event: &SyncEvent);   // 唯一事件入口
    pub fn finish(&mut self);                       // 幂等收尾
}
pub struct TerminalGuard<R: AsMut<HumanRenderer>>(...);  // Drop → finish()
```

- `LineRenderer`：现 `HumanProgress` 行为原样搬迁（含 sync.rs:611-626 的换行边界规则）。
- `BarRenderer`：持有 `MultiProgress` + `Option<ProgressBar>`（当前活动条）+ 每来源状态。
- `run_with_human_events`（sync.rs:65-131）重构：函数入口创建 renderer + guard（在 `BootstrapStarted` 渲染之前），reporter task 改为持有 renderer 的 `Arc<Mutex<..>>` 或通过 channel 所有权转移 + guard 持有FinishHandle；关键不变式：**guard 的生命周期属于命令函数本身，不依赖 reporter task 是否已 spawn**，从而覆盖 sync.rs:75/78/86 的提前返回。
- 选择逻辑：`stderr.is_terminal()` 且 `LLMUSAGE_PROGRESS` 未设置 → Bar（`stderr_with_hz(10)`），否则 Line。

## 2. 分来源展示（R2）

| 来源 | 形态 | position | message |
|---|---|---|---|
| OpenCode | spinner（steady tick） | 行数（`files_scanned` 原值） | `导入 {records_imported} 条 · {db 文件名}` |
| Codex/Claude | 确定条 length=`files_total` | `files_scanned`（重放文件） | `重放 {pos}/{len} · 导入 {records_imported} 条` |

- `SourceFinished`：`set_position(len)` → `finish()` → `MultiProgress::println(human_progress_line(event))` 落永久行（含跳过数）。
- `current_file` 恒 `None`（Codex/Claude），模板不含该字段；OpenCode 取路径末段文件名，超长截断。
- pricing 阶段：`PricingUpgradeStarted/Progress` 有 `total_events` → 确定条；其余阶段 spinner；`MigrationFinished`/`PricingUpgradeFinished`/reconcile/`LockAcquired` 落永久行。

## 3. RAII 与取消

- `TerminalGuard` 在 `run_with_human_events` 第一句创建；`Drop` 调 `finish()`：`abandon` 活动 bar（若存在）、`MultiProgress::clear()` 失败容忍、停止 tick。所有 `?` 路径自动覆盖。
- 失败：`Failed` 事件 → `abandon_with_message(错误行)`；取消：`Cancelled` → `abandon_with_message(已取消)`。
- Ctrl-C：`run_with_human_events` 内 `tokio::spawn` 监听 `signal::ctrl_c()`，触发时 cancel 注入的 token + 向 channel 发 `Cancelled`。`run_once_with_options` 改为透传该 token（现在 sync.rs:295-303 新建丢弃 token 的做法由本任务改掉；web/TUI 调用方已自行传 token 或走 `run_once_with_cancel`，不受影响——实现时核实全部调用点）。
- JSON 路径（sync.rs:133-221）同样接 Ctrl-C（只接 token，无渲染器）。

## 4. 测试策略（可注入 draw target）

- `BarRenderer::new(ProgressDrawTarget::hidden())` 驱动全事件序列（含 Failed/Cancelled/bootstrap 中途错误），断言内部状态收敛（无活动 bar、finish 幂等）。
- 非 TTY 子进程测试：stderr 接管运行 sync，断言无 0x1B。
- Ctrl-C：单元层验证 token 触发 → driver 取消语义沿用既有 parser 取消测试；终端清理由注入 target 测试覆盖，不依赖真实信号。
- 手动冒烟（PowerShell 兼容）：`cargo run -- sync`；`cargo run -- sync 2>$null`；`$env:LLMUSAGE_PROGRESS='off'; cargo run -- sync`。

## 5. 回滚

- revert 本任务即恢复 `HumanProgress` 原实现（LineRenderer 即其搬迁）；`LLMUSAGE_PROGRESS=off` 恒安全。

## 6. 已知限制

- Ctrl-C 监听在锁获取之后才安装（reporter 启动处）。bootstrap/锁等待阶段（pricing 升级可达数十秒）按 Ctrl-C 走进程默认 SIGINT 直接退出，`TerminalGuard` 不执行；该阶段为同步阻塞代码，token 亦无法中断。终端残留最多一行未收尾的 bar（indicatif 不隐藏光标，基本无害）。此为「立即退出 vs 取消无响应」的取舍，后续如需要可把 handler 提前安装。
- `LineRenderer` 相对旧 `HumanProgress` 增加了 `terminated` 收敛：重复终态事件（如双重 Cancelled）不再重复打印，属有意行为而非逐字搬迁。
- `--json-events --source X` 单来源取消时 NDJSON 以 `finished` 收尾（driver 只在多 parser 取消边界发 `Cancelled`）；human 路径由命令层 ctrl-c 任务兜底发 `Cancelled`。
