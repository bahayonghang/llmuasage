# 会话分析实现契约核对

核对日期：2026-08-03。证据来自当前仓库的查询、报表与 schema 实现。

## 会话身份

- `src/query/logs.rs` 当前逐字返回 `usage_event.session_id`、`session_label` 与
  `source_path_hash`；空值不在日志查询中合成。新增日志 `session` 过滤因此应对原始
  `session_id` 做大小写不敏感的精确匹配，并对 `session_label` 做大小写不敏感的子串匹配。
- `src/query/reports.rs::event_session_id` 的报表分组身份依次取：
  1. trim 后非空的 `session_id`；
  2. `source_path_hash`；
  3. `fallback_session_id(source, event_key)`。
- 前两级身份都加 `{source}:` 前缀，避免不同来源的同名 session 合并。fallback 对
  Codex/Claude 的四段以上 event key 取 `{source}:{parts[1]}:{parts[2]}`，其他来源保留
  原始 `event_key`。`session_id = ''` 与纯空白均视为缺失。
- schema v20 的 compact 首页索引也固化了同一字段优先级：
  `COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key)`；但 SQL
  `NULLIF` 不会 trim 空白，也不添加 source 前缀。Top Sessions 必须显式实现报表的
  trim、source 隔离和 Codex/Claude event-key fallback，不能只照抄该索引表达式。
- active duration 与报表一致：先按事件时间排序，wall-clock span 为末次减首次；active
  仅累加 `0 < gap <= 30` 分钟的相邻事件间隔，丢弃更长 idle gap。单事件会话两者均为 0。

## `usage_bucket_30m.hour_start`

- `src/store/migrations.rs` 将 `hour_start` 定义为 `TEXT NOT NULL`，不是 epoch 秒或毫秒。
- 生产写入与测试样本均使用 UTC RFC 3339 字符串，例如
  `2026-05-01T10:00:00Z`；30 分钟桶也可出现 `...T16:30:00Z`。
- 因此 Hour of Week 查询应读取 RFC 3339 文本，解析成 UTC instant，再交给
  `ResolvedZone` 转成本地星期与小时。桶按 `hour_start` 所代表的起始 instant 归属；
  不能把该列按整数 epoch 解码。
