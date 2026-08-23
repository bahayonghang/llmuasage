# 执行计划：Pi 行为信号

## 前置

- 依赖 `08-23-omp-source-split` 与 `08-23-pi-event-dimensions` 已归档。
- 读 `.trellis/spec/llmusage/backend/source-sync-contracts.md`、
  `token-accounting-contracts.md`、`dashboard-performance-contracts.md`；
  读 `src/parsers/behavior.rs` 全文。
- 读父任务 `design.md` D2（回填口径）与 R7（隐私边界）。
- 跑一次真源扫描存档，取 `tool_call_blocks`、`retry_records` 与工具名分布作为期望值：
  `python .trellis/tasks/08-23-pi-omp-usage-accounting/research/scan_pi_source.py`

## 步骤

1. [x] 先写分类期望表的单测：对扫描输出的 16 个工具名调用
       `behavior::classify_tool`，逐名断言 `ToolKind`（AC4.4）。
       **不改共享分类表**。评审已确认：`todo_write` 的共享结果是 `Edit`，
       以更新后的 PRD R4.5 为准，继续提取逻辑。
2. [x] 新增 `behavior::extract_pi_tools`：读 `message.content[]` 中
       `type == "toolCall"` 的块，取 `name`；`arguments` 为对象则直接传引用、
       为字符串则先 `from_str`、其他类型传 `None`；调 `tool_evidence`，
       按顺序填 `sequence`。单测覆盖三种形态（AC4.3）。
3. [x] `src/parsers/pi.rs` 记录回调：产出 `turn_from_tools` 与
       `tool_calls_from_evidence`，并用 `message.retryRecovery.attempt` 覆盖 `retries`
       后按既有规则重算 `one_shot`（AC4.5）。
4. [x] `PiShardOutput` 增加 `turns` / `tool_calls`；`commit_shard` 调用点替换 `Vec::new()`。
5. [x] `recent_cutoff` 分支同步过滤 turn 与 tool_call，避免孤儿行。
       单测：给定 cutoff 时 turn/tool_call 与事件数量一致（AC4.9）。
6. [x] 验证路径级重放清理：改写一个 `.omp` 会话文件后重放，断言旧 `path_hash` 的
       turn/tool_call 被清理、不重复累积（AC4.10）。既有机制在
       `src/store/sync_writer.rs:656`，只做验证，不新增机制。
7. [x] 集成测试：临时 home 下构造含 `toolCall` 块（对象 arguments）与
       `retryRecovery` 的 `.omp` 会话，断言 turn、tool_call、`tool_kind` 分布、
       `retries`、`project_hash` 落库。
8. [x] 隐私断言：单测确认 `safe_preview` 长度 <= 120，且不含 `toolResult` 记录的
       `content`（AC4.8）。不修改 `safe_tool_preview`。
9. [ ] 看板确认：`just serve` 后在行为面板选 `omp` 源，确认有数据、无报错（AC4.11）。
       实现侧已加 tempfile 集成测试调用 `activity_breakdown` / `tool_breakdown`
       （source=omp）；live home / `just serve` 由 supervisor 验收。

## 验证命令

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
cargo run -- sync --rebuild --source omp     # 历史回填（父任务 R8）
just serve
just ci
```

本机数据校验（只读查询，对照当次扫描输出）：

```sql
SELECT COUNT(*) FROM usage_tool_call WHERE source='omp';
-- 期望：等于扫描的 tool_call_blocks（AC4.2）
SELECT tool_kind, COUNT(*) FROM usage_tool_call WHERE source='omp' GROUP BY 1;
-- 期望：与按 R4.5 映射折算的工具名分布一致
SELECT COUNT(*) FROM usage_turn WHERE source='omp' AND retries >= 1;
-- 期望：等于扫描的 retry_records（AC4.6）
SELECT COUNT(*) FROM usage_turn WHERE source='omp' AND (project_hash IS NULL OR project_hash='');
-- 期望：0（AC4.7）
SELECT COUNT(*) FROM usage_tool_call WHERE source='omp' AND LENGTH(safe_preview) > 120;
-- 期望：0（AC4.8）
```

## 评审门

- 步骤 1 的分类断言先通过，再写提取逻辑，避免分类返工或误改共享表。
- 步骤 5、6 是数据一致性风险点，通过后再做看板确认。

## 回滚点

- 纯代码改动，`git restore` 可回退。
- 已写入的 turn / tool_call 行用 `sync --rebuild --source omp` 清理。
