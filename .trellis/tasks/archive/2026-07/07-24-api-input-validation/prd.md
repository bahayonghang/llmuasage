# API 输入校验与并行度上限（RES-001/API-001/CONTRACT-001）

## Goal

把 sync/查询入口的输入从"宽松字符串 + 静默退化"改为 typed 校验：非法输入返回 400 与稳定 error code，parallelism 有 service-side 硬上限，`recent_days` 语义与实现一致。

## 覆盖发现（已核实）

- **RES-001（P1）**：`src/sync/job_registry.rs:139-141` `parallelism: Option<usize>` 接受任意值，注释自述"低于 1 由 runner 忽略"；`src/commands/sync.rs:419-427` runner 仅 `.max(1)`，无上限；`src/parsers/claude.rs:143-197` 按该宽度批量 `spawn_blocking`。极大值可制造大量 blocking task 与 I/O；public guard 绕过后可远程触发（与 SEC-001 组合成 DoS）。
- **API-001（P2）**：`src/sync/job_registry.rs:338-343` unknown `source` 经 `and_then(parse_id)` 变 `None`，而 `None` 在 runner 中代表**全部 parser**；`src/web/mod.rs:1480-1527` unknown window 被静默忽略。typo 本应 400，却扩大为全量 sync/全量查询。
- **CONTRACT-001（P2）**：`src/sync/job_registry.rs:135-136` `recent_days` 注释自述"M0 只存 option"；`src/parsers/driver.rs:46-55,129-134` 仅在完整 parse 后 mark `RecentReady`，未限制扫描范围。用户以为是 bounded import，实际执行全量扫描；`docs/reference/cli.md` 的命令语义不可信。

## Requirements

1. `normalize_parallelism`：默认 `min(cpu, 4)`，合法范围 `1..=32`（或按硬件策略），越界返回 400/CLI error，不静默 clamp 到边界之外的语义。
2. transport DTO 使用 serde typed enum 或显式 validator：unknown source/window/timezone/date 一律 400 + 稳定 error code，不退化为 None/全量。
3. `recent_days`：真正实现时间/offset pruning，或改名为如实描述（如 `recent_ready_window`）并在 response 中返回 `applied=false`；文档同步。
4. CLI 与 Web API 使用同一校验层，错误码 schema 统一。

## Acceptance Criteria

- [ ] parallelism 0、33、`usize::MAX` 均返回 400/CLI error（在旧实现上稳定失败）。
- [ ] unknown source/window 返回 400，不触发全量 sync/查询；property/fuzz 测试覆盖。
- [ ] `recent_days` 行为与文档、CLI help、API payload 三者一致。
- [ ] 合法输入路径全部不回归（现有集成测试通过）。

## Notes

- 审计报告 §1.2 RES-001 深挖含 `normalize_parallelism` 草图；§3.2.3。
- parallelism cap 是审计"当天阻断项"之一，可先行以小 diff 落地，再做完整 typed validation。
