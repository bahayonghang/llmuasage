# 按业务风险补充核心测试覆盖

## Goal

在不改产品语义的前提下，按业务风险补齐现有实现中未覆盖的核心路径：SQLite 事务回滚、查询/日志边界、Web 权限与参数校验、sync 输入边界、remote host 校验、订阅 HTTP 状态映射。每完成一个模块跑对应测试，最后跑完整 Rust 测试套件。

## Background

仓库当前约 933 个 Rust `#[test]`（src 729 + tests 204）。写围栏窃锁、schema 拒绝、sync job 校验、public 路由 allowlist、explorer 非法 metric、logs 非法 cursor 已有测试。

静态对照后，下列现有行为没有对应用例。完整清单见 `research/coverage-gap-inventory.md`。

## Requirements

- R1. Store：`write_transaction` 闭包失败时已插入行不得提交；`reset_usage_data` 清空用量/行为表并保留 `run_log` 与 `integration_install`；损坏的 cursor JSON 按现有 `.ok()` 语义加载为空/默认，不 panic。
- R2. Query：空白 `host_id`/`model`/`project_hash` 不进入 SQL；`until` 无后继日期时不加 until 子句；logs `page_size` 0→50、>500→500；分页产生并消费 `next_cursor`；空 cursor 字段拒绝。
- R3. Web：public dashboard filter 丢弃 `host`/`host_id`；`POST /api/diagnostics/forget` 缺 `source` / 未知 `source` 返回稳定 400 code；explorer 非法 `granularity`/`group_by`/`token_type` 返回 `invalid_query`；explorer `limit` 被夹到 `1..=50`。
- R4. Sync：`ValidatedSyncRequest` 拒绝 `parallelism=0` 与 `recent_days=3651`，接受 `recent_days=3650`。
- R5. Remote：`validate_new_host_id` 拒绝空 id、与 source 名冲突、已注册重复 id。
- R6. Subscription：`status_error` 对 401/403 给出 token-rejected 文案，对其余状态给出通用失败文案。
- R7. 实现顺序按 R1→R6。每个模块补完后只跑该模块测试。全部模块完成后跑 `cargo test --all-features -- --test-threads=1`。
- R8. 不修改生产逻辑、错误码、HTTP 状态或 SQL 语义。测试失败只能说明现有行为与文档/代码注释不一致，此时停下来改规划，不静默改产品。

## Acceptance Criteria

- AC1. `Store::write_transaction` 在闭包 `Err` 后目标行计数仍为 0（R1）。
- AC2. `reset_usage_data` 后 `usage_event`/`usage_turn`/`usage_tool_call` 为 0，且预先写入的 `run_log` 与 `integration_install` 行仍在（R1）。
- AC3. 损坏 `last_total_json` 的 file cursor 加载后 `last_total` 为 `None`（R1）。
- AC4. 仅含空白的 host/model/project 过滤器生成的 SQL 不含对应列谓词（R2）。
- AC5. logs `page_size=0` 返回至多 50 条；`page_size=1000` 至多 500 条；`page_size=1` 时第二页经 `next_cursor` 取到下一行；空字段 cursor 解码失败（R2）。
- AC6. `public_dashboard_filter_from_params` 在传入 `host` 或 `host_id` 后 `host_id` 为 `None`（R3）。
- AC7. loopback `POST /api/diagnostics/forget` 无 source → `missing_source`；未知 source → `unknown_source`（R3）。
- AC8. `GET /api/explorer` 对非法 granularity/group_by/token_type 返回 400 `invalid_query`（R3）。
- AC9. explorer `limit=0` 与 `limit=999` 经 query sanitize 后夹在 `1..=50`（R3）。
- AC10. `ValidatedSyncRequest::new` 对 `parallelism=0`、`recent_days=3651` 返回对应稳定 error code；`recent_days=3650` 成功（R4）。
- AC11. `validate_new_host_id("", …)`、`validate_new_host_id("codex", …)`、已存在 host 均返回 `LlmusageError::ConfigInvalid`（R5）。
- AC12. `status_error("Codex", 401)` 文案含 stored access token rejected；`status_error("Codex", 500)` 不含该短语（R6）。
- AC13. 每个模块的聚焦测试与最终 `cargo test --all-features -- --test-threads=1` 退出码为 0（R7）。
- AC14. `git diff` 不含 `src/**` 生产逻辑变更，仅测试与本任务规划文件（R8）。

## Out of Scope

- 把 dashboard 查询的未知 `source`/`window`/`timezone`/`since` 从静默退化改成 400。
- 为提高行覆盖率测试 TUI 绘制、主题、格式化、help 文案、薄命令包装。
- 新增 JS 仪表盘渲染测试、`cargo llvm-cov` CI 门槛、对 live-home 库或真实网络配额的测试。
- 修改 write fencing、public allowlist、sync job 校验等已有测试所覆盖的行为。
