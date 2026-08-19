# 执行计划：看板立即同步反馈与重建风险闭环

## 前置阅读

- `.trellis/spec/llmusage/backend/source-sync-contracts.md`（命令中心信号、usage-import 窗口、run_log 契约）
- `.trellis/spec/llmusage/backend/write-fencing-contracts.md`（持锁期间的 Store 写入）
- `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`（终态 hook、同步后刷新与缓存失效）
- `.trellis/spec/llmusage/backend/tui-presentation-contracts.md`（TUI tone、状态文案与颜色槽位）
- `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md`（ARCH-002 与本地 CI 命令）
- `docs/agents/domain.md`

## 环境约束

- 手工修改统一使用 `apply_patch`，保留现有 JS 单引号与格式；修改后运行 `node --check`，不引入额外格式化依赖。
- 改动 `.rs` 后运行 `cargo fmt`，再检查 scoped diff 与 `git status`，避免格式化噪声或无关文件混入。
- 不修改或读取用户用量数据库作为测试夹具；全部回归使用 `tempfile` / 现有 Fixture。

## 步骤

### S1 — 统一 usage-import 恢复与 Job 记账（R1 / R2 / R3 / R9）

1. `src/store/run_log.rs`：以现有 `USAGE_IMPORT_COMMANDS` 为唯一命令集合，新增 `recover_running_usage_import_runs()`，内部恢复 `sync`、`sync --rebuild`、`hook-run`。
2. `src/commands/sync.rs`：把所有手写 `recover_running_runs([...])` 替换为统一入口，先修复 CLI 自身遗漏 `sync --rebuild` 的问题。
3. `src/sync/types.rs`：提取共享 summary 文本函数（`sources= seen= inserted_delta= stored_events=`），CLI 与 Job 路径共用，输出字节格式不变。
4. `src/sync/job_registry.rs::run_job`：持锁 + bootstrap 后执行恢复、`record_run_start`、executor 与 `finish_run`；完成记账后才能释放 heartbeat / lock。锁前取消不写 run row；执行期取消写 `cancelled`。
5. `record_run_start` / `finish_run` 失败时发送失败事件并把 JobSnapshot 标为 failed；executor 错误与收尾错误同时存在时保留两者的上下文，不把任务伪装成 completed。
6. `src/store/mod.rs`：`RunRecord::counts_as_failure` 排除 `cancelled`，保留 `aborted` 为失败证据。

验证：

- `cargo test --all-features job_registry -- --test-threads=1`
- `cargo test --all-features run_log -- --test-threads=1`
- `cargo test --locked --all-features --test architecture_dependencies`

### S2 — 共享命令中心与 TUI 中性语义（R4 / R5）

1. `src/query/mod.rs::sync_command_center_with_diagnostics`：移除 tone / headline / reason 中的 `lossy_rebuild_risk` 分支；来源行 `status` 不再产生 `rebuild_risk`。
2. 新增 `SyncRiskSourcePayload` 与 `SyncSafetyPayload.risk_details`，直接从已筛选的 `diagnostics.by_source` 构建；保留并校验 `risk_sources == risk_details[].source`。
3. `src/tui/panels/sync_status.rs`：仅有重建保护事实时显示 ready/neutral；来源行不因布尔值整行变 warning；显式 rebuild-risk 事实与 cancelled last run 使用 neutral 样式。锁忙、失败、来源错误和真正解析故障仍为 warning。
4. 更新 query / TUI / `src/web/mod.rs` 既有断言：成功 + risk → ready/good 且 safety 事实保留；失败和 busy 的优先级不变；旧 rebuildRisk key 映射仍可处理构造的旧 payload。

验证：

- `cargo test --all-features sync_command_center -- --test-threads=1`
- `cargo test --all-features sync_status -- --test-threads=1`

### S3 — 看板中性事实与完成态过渡（R6 / R7 / R8）

1. `src/web/assets/render/sync-command-center.js`：`safetyLine` 去掉风险前缀；source card / segmented bar 不再按重建布尔值取 warn；按 `safety.risk_details` 渲染中性事实；`centerWithJobOverlay` 增加 completed 分支。
2. 同一文件补齐完成详情：`summaryRows` 显示终态 `finished_at`，`secondaryStatus` 为 completed 显示本次事件与完成时间，不回退到旧 `last_run`。
3. `src/web/assets/data/derive.js`：规范化 `risk_details`；`lossy_rebuild` 洞察 tone 改为 `neutral`。
4. `src/web/assets/copy.js`：新增中性事实和 completed/finished 中英文文案，避免暗示已知删除者；改写 `insights.lossy_rebuild`，保留旧 rebuildRisk key。
5. `src/web/mod.rs` 中的资产内容断言按实际改动更新。

验证：

- `node --check src/web/assets/render/sync-command-center.js`
- `node --check src/web/assets/data/derive.js`
- `node --check src/web/assets/copy.js`
- `node --test scripts/tests/dashboard-render-lifecycle.test.mjs`

### S4 — 新增回归矩阵（AC1–AC11）

1. `src/store/run_log.rs` 或邻近 store 测试：分别预置 `sync`、`sync --rebuild`、`hook-run` 的 running 行，统一恢复入口全部改为 `aborted`；非 usage-import running 行不受影响。
2. `src/sync/job_registry.rs`：成功写 success + summary；executor 失败写 failed；执行期取消写 cancelled 且不计健康失败；锁等待期取消无 run row、不会延迟取得锁。
3. 同一 JobRegistry 测试通过失效/窃取测试 permit 令 `finish_run` 返回 `LockLost`，断言 JobSnapshot/事件为 failed 而非 completed，且下一次统一恢复能处理遗留 running 行。
4. `src/web/mod.rs` 集成测试：`POST /api/jobs` 完成后取命令中心，断言 `last_run.finished_at` 为本次运行、ready/good、`lossy_rebuild_risk == true` 且 `risk_details` 计数正确。
5. query/TUI 测试：risk-only、failed+risk、busy+risk、cancelled last run 四个组合覆盖共享优先级和中性/警告样式边界。
6. `scripts/tests/dashboard-render-lifecycle.test.mjs`：通过导出的 `renderSyncCommandCenter` 与 DOM stub 连续模拟“旧 payload + completed overlay → 新 payload + overlay → 新 payload 无 overlay”，每一步断言 good/ready、本次 `finished_at`、来源非 warn 和中性保护事实。
7. 运行现有 rebuild guard 回归，证明缺失文件下仍拒绝无 `--allow-lossy-rebuild` 的重建。

### S5 — 文档与规范（R10）

1. `.trellis/spec/llmusage/backend/source-sync-contracts.md`：更新共享命令中心优先级、usage-import 恢复集合、JobRegistry run_log、锁前/执行期取消、`counts_as_failure` 和测试清单。
2. `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`：记录 completed overlay → reload → clear 的无闪回过渡与 `risk_details` additive payload。
3. `.trellis/spec/llmusage/backend/tui-presentation-contracts.md`：记录重建保护事实 neutral、真正同步故障 warning 的颜色/状态边界。
4. `docs/zh/safety/index.md` 与对应英文页：说明重建风险是 `sync --rebuild` 守卫事实，普通同步保留归档；保留 CLI 与现有本地 HTTP forget 入口的准确说明。
5. `README.md` / `README.zh-CN.md`：仅当现有看板同步行为说明受影响时更新，不新增重复安全章节。

### S6 — 全量校验

依次运行：

1. `cargo fmt --check`
2. `python scripts/ci-rust.py`
3. `node --test scripts/tests/dashboard-render-lifecycle.test.mjs`
4. `npm --prefix docs run docs:build`（文档改动后的快速门禁）
5. `just ci`

任何失败都保留证据、就地修复并重跑对应门禁；最终必须重新运行完整 `just ci`。

## 审查关口

- G1（S1 后）：确认 CLI 与 JobRegistry 都只调用统一 usage-import 恢复入口，且集合含三个命令。
- G2（S1 后）：确认正常终态在锁释放前完成 `finish_run`；注入收尾失败时 Job 不会报告 completed。
- G3（S2/S3 后）：确认共享 payload、TUI 与 Web 都采用中性重建语义，安全布尔值与计数没有丢失，真正失败/锁忙仍为 warning。
- G4（S4 后）：对 D1、D2、D3 各保留至少一条“改动前失败、改动后通过”的回归证明；三阶段渲染测试必须覆盖旧 payload，不能只测最终状态。

## 回滚点

- S1 可独立提交，但统一恢复入口、CLI 调用方与 JobRegistry 记账必须同批落地。
- S2+S3 必须作为一个语义单元提交，避免共享 payload、TUI、Web 任一层仍保留旧告警。
- 任一步骤破坏 `sync --rebuild` 守卫、锁前取消 SLA 或写围栏（AC7/AC9/AC11）即回滚该语义单元重做。
