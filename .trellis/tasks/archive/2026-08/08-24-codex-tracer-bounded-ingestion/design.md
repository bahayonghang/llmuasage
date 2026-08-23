# Design

## Boundary

新增一个 Codex-specific record decoder，复用现有 `BoundedJsonlReader`：

```text
file bytes + start_offset + CancellationToken
  -> bounded JsonlRecord { start_offset, end_offset, value }
  -> CodexEnvelopeRecord { timestamp, kind, payload, durable_end }
     -> main parser state -> UsageEvent/UsageTurn/UsageToolCall
     -> tracer state -> CodexTracerEvent batch
```

decoder 不持 Store、pricing、project resolver 或 tracer schema；它只解析共同 envelope 结构并保留 record boundary。

## Tracer State

在 tracer schema 内新增 file-state 表，至少包含 canonical path hash、file identity/fingerprint、durable byte offset、line counter、decoder session context、updated_at。完整路径仍按既有 tracer contract 处理，新增状态/日志不得输出路径。

状态与对应 event batch 在同一 SQLite transaction 提交。truncate/replace 检测后，按 `source_file` identity 删除该文件派生的 tracer rows，再从 0 重放；其他文件不动。

## Batch And Thread Linkage

- 默认 batch 上限 2,048 events，允许测试注入更小值；该值由 100k release
  harness 在逐 batch durable 事务约束下校准，替代规划期的 1,000 初值。
- batch 不等待所有文件完成；每个完成 batch 立即 upsert。
- previous/next 使用稳定 `(thread_key, timestamp, record_id)` 排序在数据库内或 file-finalization 阶段更新，不能要求全历史 Vec。
- clean rebuild 与 incremental result 必须逐字段等价。

## Compatibility

旧 public parser functions 作为有界 collector wrapper 保留，但主命令/refresh 不再调用会返回全文件 Vec 的 convenience path。`CodexTracerStore::upsert_events` 继续可用，并下沉复用 batch transaction。

## Performance Harness

生成固定多文件 corpus；seed/生成时间不计入 import timing。采集进程 peak working set、wall p50/p95、record count、batch high-water、DB rows/bytes。Windows 用内置系统计数或已有脚本能力，不为此引入运行时依赖。

## Rollback

先落兼容 decoder 与 fixtures，再落 state migration，再切命令/refresh，最后删除旧内部行循环。任一阶段失败可回到旧 wrapper；schema migration 保持 additive，rollback 不删除用户 DB。
