# Design: Sync Job Contract Closure

## Validation Boundary

- transport DTO 先解析到 typed `SyncRequestInput`，再由唯一 validator 生成 `ValidatedSyncRequest`。
- `JobRegistry` 只接受 validated request，或其 public convenience API 内部强制调用 validator。
- source 解析使用 strict enum/error；`None` 只代表调用方明确选择 all。

## Recent Window

- cutoff 使用 UTC instant，来源 parser 将事件时间标准化后比较。
- discovery 可用文件 metadata 做保守优化，但不能仅凭 mtime 排除可能追加的文件。
- durable cursor 继续按完整记录推进；窗口过滤不能破坏未来全量 sync 的可恢复性。

## Event Semantics

- `RecentReady` 只在所有 requested source 完成 recent window 后发出。
- full continuation 如存在，必须是显式阶段，不能把“recent”当无效标签。

## Compatibility

- 合法 source 和默认 all 行为不变。
- 非法字符串从静默全量改为显式错误，属于安全/契约修正。

