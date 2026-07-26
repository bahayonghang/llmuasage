# Design: Bounded JSONL Reader

## Reader Contract

- reader 维护 `record_start`, `bytes_seen`, `durable_offset`，仅在完整换行 record 被消费后推进 durable offset。
- 使用 bounded buffer/`fill_buf` 分段搜索换行；超过 limit 后丢弃到下一个 record boundary 并发出 `LineTooLong` issue。
- parse callback 返回 accepted/malformed 分类；reader 负责统一 issue 计数和采样预算。

## Cancellation

- blocking closure 持有 cloneable cancellation token。
- 每处理固定 bytes/records 检查 token；超长行的 discard loop 也必须检查。
- async driver 等待 worker drain，再发 terminal cancelled event。

## Diagnostics

- `ParseIssues { malformed_lines, oversized_lines, samples }` 并入 `SourceSyncStats`/diagnostics。
- sample 只含 source、path hash/安全路径表示、offset 和 issue kind，不含整行文本。

## Reuse Boundary

- source parser 只提供 JSON-to-event callback；cursor、limit、取消和 issue 逻辑由 reader 统一。
