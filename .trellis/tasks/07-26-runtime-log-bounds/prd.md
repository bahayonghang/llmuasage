# 运行期日志有界化与丢弃观测

## Goal

让持续运行的 `serve`/TUI 进程在不重启的情况下保持日志磁盘占用有界，并暴露 non-blocking writer 丢弃日志的可观测信号。

## Confirmed Evidence

- `src/runtime/logging.rs:79` 仅在启动时清理旧日志。
- `src/runtime/logging.rs:81` 的 daily rotation 不能限制当天单文件大小。
- 当前没有 dropped-message counter/diagnostic。

## Requirements

- rotation 同时受 size 和 retention 约束；长进程内持续执行，不依赖重启清理。
- 明确总量预算、单文件预算和保留窗口，默认值适合本地 CLI/serve。
- non-blocking writer 发生丢弃时递增原子计数，并通过 diagnostics/status 暴露。
- rotation/cleanup 失败写入 fallback diagnostics，但不得递归写日志形成循环。
- tail 查询继续按尾部复杂度读取，不因分片退化为扫描全部历史。

## Acceptance Criteria

- [ ] 单进程持续写入超过多个 size boundary 后，总日志量稳定在配置预算内。
- [ ] 当天单文件不会无限增长，重启不是触发 rotation 的必要条件。
- [ ] 压满 non-blocking queue 的 deterministic test 可观察 dropped counter 增长。
- [ ] 多分片 tail 返回正确最新顺序，复杂度与请求 tail 量近似相关。
- [ ] Windows 文件占用/rename 失败有明确降级，不导致 logger panic。

## Out of Scope

- 不新增远程 telemetry backend，不在此任务决定 public API 的日志暴露策略。
