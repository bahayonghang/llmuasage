# Design: Windows Atomic Integration Replace

## Preferred Design

- Unix 继续使用 sibling temp + fsync + rename，并补 parent directory sync（平台支持时）。
- Windows 目标存在时通过 `ReplaceFileW` 原子替换；目标不存在时使用同目录 rename。
- 用 target-specific `windows-sys` 直接依赖提供 API/常量，避免手写 ABI；该依赖已获得用户授权。

## Recoverable Action Protocol

1. 读取 old digest/metadata，并创建唯一 backup 或预写 pending action。
2. 写入并 durable flush sibling temp。
3. 原子 replace。
4. 写 completed action。
5. 第 4 步失败时使用 backup 原子恢复；恢复失败返回包含 recovery path 的 typed error，不能报告成功。

## Compatibility

- `write_file_atomic` 对调用方保持同一高层接口，但内部返回更精确的 replace/recovery error。
- 不改变外部 config 格式或 integration 探测语义。

## Security

- temp/backup 名称不可预测且 `create_new`，避免覆盖现有文件。
- 恢复信息只记录路径摘要/必要元数据，不泄露配置内容。
