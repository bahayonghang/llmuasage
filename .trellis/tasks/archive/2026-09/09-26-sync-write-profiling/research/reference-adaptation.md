# 参考机制的实施适配

日期：2026-09-28。对应本任务 P5、父任务 R8/AC6。参考仓库更新与逐项源码分析见父任务 `research/upstream-comparison.md`。本文件说明实施采用的边界，不提供三个项目之间的性能排名。

## 固定参考版本

- ccusage：`732c7a6362f3d86a4992d2ad7071b6532161a396`。
- tokscale：`1d9a9395418efc6952944b794097935d7d6fa1e8`。

以下 C 路径相对于 `ref/repo/ccusage/`，T 路径相对于 `ref/repo/tokscale/`。这些提交已在原始规划前更新，本次续作沿用同一比较基准。

## 采用机制与反例

| 参考机制及源码位置 | llmusage 适配点 | 必须保留的边界与反例 |
| --- | --- | --- |
| C：逐行筛选 Codex 记录，`rust/adapters/codex/src/parser.rs:224–248,475–548` | `src/parsers/codex.rs` 保留既有有界 reader；超长且可确定为非 usage 的记录使用 `oversized_non_usage_record`。`src/commands/sync_summary.rs` 显示计数、8 条样本上限及省略数。 | 不照搬静默丢弃；未知超长记录、坏 JSON、usage accounting 仍分别诊断。状态记录可能影响累计基线，不能只保留 `token_count`。 |
| C：Grok 原生 usage 与字段结构，`rust/adapters/grok/src/parser.rs:67–102,207–241`；T：原生 usage/fallback 读取，`crates/tokscale-core/src/sessions/grok.rs:174–259,499–518` | `src/parsers/grok.rs` 保留 `usageIsIncomplete` 对应的可用原生值与 `usage_incomplete` 定位。 | 不用默认 0 证明字段完整；原生 usage 与 session fallback 排他，不复制 T 的混合策略。保持一条 usage 对应一条 event，不转换 `costUsdTicks`。 |
| C：区分 SQLite 读取错误、`page_count=0` 的未初始化零页数据库和成功，`rust/adapters/antigravity/src/parser.rs:143–171,290–331`；T：观察 DB/WAL，`crates/tokscale-core/src/message_cache.rs:402–418` | `src/parsers/antigravity.rs` 在全部选中产品覆盖检查已阻断时跳过 usage decode；`src/domain/source_diagnostics.rs` 独立保存源级原因。 | 发现与 fingerprint 仍执行。部分阻断保留跨 root 解码与强身份归属。缺失、不可读、范围外分别处理；`--allow-lossy-rebuild` 不豁免访问错误，不自动重建历史。 |
| T：先按受影响 shard 分组 dirty keys，成功后再清状态，`crates/tokscale-core/src/message_cache.rs:2582–2606,2675–2702` | 写入研究定位 `SyncRunWriter::reset_file_events_batch_tx`，先测量重复扫描，再评估现有索引的选择。沿用现有 temporary key table、prepared statements 与单 writer。 | T 持久化二进制 cache；llmusage 同一事务写 events、buckets、facts、cursor。不得通过先提交 cursor、拆分 reset 事务或增加 writer 缩短测量时间。具体 SQL 候选须独立通过 P3/P4。 |

## 未采用机制

1. C 的按文件大小分配 worker（`rust/adapters/common/src/lib.rs:49–80`）没有形成当前 writer 热点的证据。本任务不增加 parser 并发或改变 Claude 项目重放分组。
2. T 的 metadata/samples fingerprint（`crates/tokscale-core/src/message_cache.rs:688–709`）没有证明本项目需要第二份 cache。本任务不添加 durable cursor、hash cache 或 samples-only 内容等价判断。
3. T 的 Codex append 在读取尾部前调用 `codex_prefix_matches`（`crates/tokscale-core/src/lib.rs:1966–1974`），该函数调用 `hash_prefix`（`crates/tokscale-core/src/message_cache.rs:3162–3169`）。本任务不复制全前缀读取，也不将该路径称为纯尾部 I/O。
4. T 的 Antigravity token 字段解释（`crates/tokscale-core/src/sessions/antigravity_cli.rs:312–325,374–385`）与 llmusage 已核实的 descriptor 不同。本任务保留原生 protobuf 字段、typed request time、attempts 优先与历史保护。

## 验证范围

诊断和预检的定向回归、完整门禁结果分别由前两个子任务的 `check-report.md` 保存。写入任务采用固定 seed 与完全相同的初态/输入，对比完整持久状态、幂等、失败/取消和 source/host 隔离。主场景、历史放大、append、重复记录、共享 bucket 与多 host 共享 path 的结果由正式 A/B 报告保存。

原始运行的 `WRITE=105193 ms` 属于用户安装版 1.3.0，其构建提交未知。合成 writer 基准不包含相同真实输入，不能据此推导该次 sync 优化后的秒数。参考项目没有执行等价的 SQLite materialization 工作，不能用上游耗时替代本任务的 20%/10% 验收。
