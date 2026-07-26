# Design: Runtime Log Bounds

## Rotation Model

- 使用 size-aware writer 生成序号分片，并保留日期信息用于 retention。
- 写入路径在达到 size threshold 时滚动；后台/低频 maintenance 删除超出 count/age/total-byte 预算的最旧分片。
- cleanup 必须序列化，避免多个 writer 同时 rename/delete。

## Dropped Metrics

- 包装 non-blocking error/dropped callback，维护 `AtomicU64` 总量与最近快照。
- diagnostics 读取快照，不通过 logger 自己报告 dropped 事件。

## Tail

- 从最新分片向旧分片反向收集直到满足 line/byte limit。
- 对 UTF-8 边界和 partial final line 保持现有展示语义。

## Platform Behavior

- Windows rename/delete 被占用时保留当前分片并延迟重试；预算可短暂超出一个分片，但不能永久无界。
