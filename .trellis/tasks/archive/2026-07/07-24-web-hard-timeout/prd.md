# Web 查询硬超时与后台收尾（PERF-002）

## Goal

使配置的 Web 查询 timeout 成为真实的响应延迟上限：超时后立即返回，清理工作交给后台 supervisor，不再阻塞请求。

## 覆盖发现（已核实）

- **PERF-002（P1）**：`src/web/mod.rs:1030-1143` `load_via_dashboard_with_timeout`：三处 timeout 分支（1101、1117、1128 行）在返回超时错误**之前**执行 `let _ = task.await`，等待 blocking task 结束；interrupt handle 尚未建立时也等待。非 SQLite 阶段（`Dashboard::open`、文件系统、JSON 序列化、Rust 后处理）或未响应 interrupt 的工作可无限拖延响应，并持续占用 query permit。

## Requirements

1. timeout 触发后立即返回 504/timeout 错误，不 await blocking task。
2. blocking task 自持 query permit（permit 移入闭包），直到任务真正结束才释放，避免 timeout 后 permit 提前归还导致 orphan 任务无限堆积。
3. 引入 task supervisor：记录 orphan 任务的实际停止时间与 stuck count；暴露 `dashboard_query_inflight`、`timed_out_tasks`、`orphan_duration_ms` 观测值。
4. Rust 端后处理大循环增加 cooperative cancellation（每 N 行检查）；`Dashboard::open` 等阶段纳入 budget 或单独 timeout。

## Acceptance Criteria

- [ ] 注入忽略 SQLite interrupt、sleep 60s 的 blocking closure，HTTP 请求在配置 timeout ±100ms 内返回（在旧实现上稳定失败）。
- [ ] permit 直到任务真正结束才释放；并发 inflight 不超过 permit 总数。
- [ ] timeout 后 orphan 任务可观测（日志/诊断接口含 orphan duration 与 stuck count）。
- [ ] 正常快查询路径行为与延迟不回归。

## Notes

- 审计报告 §1.2 PERF-002 深挖；§3.2.4（前置依赖 query metrics，可在本任务内一并补齐）。
- 与 07-24-explorer-sql-topn 可并行。
- 复杂任务：启动前需补 design.md + implement.md。
