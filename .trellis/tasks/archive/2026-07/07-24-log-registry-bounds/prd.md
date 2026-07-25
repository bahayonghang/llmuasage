# 日志与 JobRegistry 资源有界化（OBS-001、REL-001）

## Goal

让长期运行的 `serve` 进程在日志体积、日志 tail 复杂度与 job registry 内存三个维度上持续有界。

## 覆盖发现（已核实）

- **OBS-001（P2）**：`src/runtime/logging.rs:72-75` 10 MiB 检查（`enforce_log_size_limit`）只在进程启动时执行，随后使用 `tracing_appender::rolling::never`——长期 serve 日志可无限增长；`142-180` 行读取最近日志从文件头遍历到 EOF，`logs/diagnostics` tail 延迟 O(file size)。
- **REL-001（P2）**：`src/sync/job_registry.rs:189-314` 每个成功或 rejected job 都插入 DashMap；只有 `list_recent()` 内才 prune（302 行 `.position` 为 O(n²)），而 Web 层没有调用 list_recent 的路由（仅测试调用）。长期 serve + 反复 rejected starts 导致 registry 无界增长。

## Requirements

1. 日志：size/daily rotation + retention（如 10 MiB × 3 或 7 天）；对 non-blocking writer 丢弃的日志暴露 metric。
2. tail：反向读取或维护索引，使 `logs`/`diagnostics` 的 tail 复杂度为 O(tail bytes/lines)。
3. JobRegistry：terminal transition 时维护 bounded deque/TTL（如 ≤100 条或 24h TTL，可配置）；active job 永不淘汰；清理复杂度 O(1)/O(log n)，不放在查询路径的副作用里。

## Acceptance Criteria

- [ ] 长时间写入测试：日志文件总量稳定在配置上限内。
- [ ] 100 MiB 日志文件的 tail 延迟与文件大小无关（benchmark 断言）。
- [ ] 10k 次 rejected starts 后 registry map 仍有界（load test，在旧实现上稳定失败）。
- [ ] 旧 job URL 在保留窗口内仍可查询，窗口外 404 且文档说明。

## Notes

- 审计报告 §1.1 OBS-001/REL-001；量化目标见 §4（Log file size、JobRegistry terminal entries、Log tail complexity）。
- 轻量偏中等任务：PRD-only 可行，若改动 registry 数据结构建议补简版 design.md。
