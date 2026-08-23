# Codex Tracer 有界增量摄取与共享解码边界

## Goal

保留独立 `codex-tracer.db` 和专用 UI，以共享的有界 Codex JSONL record/envelope 边界替换 Tracer 的第二套无界行读取，并把全量内存累积改为可恢复、可取消、按文件有界批次提交的增量摄取。

## User Value

拥有大量 Codex rollout 历史时，首次构建、刷新和重启 Tracer 的内存不再随全部事件线性累积；追加、改写、超长记录和中断后重试保持可预测且不丢调用链。

## Confirmed Facts

- 独立 tracer 数据库是 `.trellis/spec/llmusage/backend/codex-tracer-contracts.md:468-486` 的明确决策，本任务不合并数据库。
- `src/commands/codex_tracer/mod.rs:68-94` 先累积全部文件事件，再调用一次 `upsert_events`。
- `src/commands/codex_tracer/parser.rs:61-110` 使用 `BufRead::lines()` 和 `Vec`；`byte_offset` 没有用于 seek，final `line_number` 取最后 usage event。
- 主 Codex parser 在 `src/parsers/codex.rs:560-646` 已拥有 4 MiB bounded reader、durable offset、oversized/EOF/cancellation 语义。
- Tracer spec 的 10k+ benchmark、SQLite query optimization 和真实 integration tests 仍未完成（同 spec:456-461,607-611）。

## Requirements

- R1：先建立不读取真实用户数据的 deterministic corpus，覆盖 valid usage、非 usage、malformed、10 MiB oversized、UTF-8、无换行 EOF、append、truncate/replace、多文件线程链和取消。
- R2：共享层只拥有 bounded record 与 Codex envelope 解码；主 parser 继续拥有 `UsageEvent`/cursor/accounting，Tracer 继续拥有 `CodexTracerEvent`/thread linkage/schema。
- R3：Tracer 必须按 durable byte boundary 和文件 identity 持久化 state；第二次 unchanged refresh 不从头跳行，append 只读新增字节，replace 只重建受影响文件。
- R4：摄取以固定上限 batch 提交，不在单文件或多文件层持有全部 events；thread previous/next 关系必须能跨 batch/file 正确收敛。
- R5：fresh、旧数据库升级、重复升级、`--rebuild` 和失败回滚必须安全；不得静默删除既有 tracer 数据库。
- R6：取消、malformed/oversized 和部分文件失败必须留下可重试的 prior durable state；不得把未完成 batch 标成成功。
- R7：保留 `parse_codex_jsonl_for_tracer`、`parse_codex_jsonl_with_state`、`CodexTracerStore`、CLI 参数、API payload 和 `codex-tracer-v1` schema 的兼容入口，旧入口只能包装新引擎。
- R8：性能 evidence 同时报告 wall time、peak RSS、records read、rows written、batch peak 和数据库大小；不记录路径、session/thread id 或事件内容。

## Acceptance Criteria

- [x] A1/R1,R2：共享 corpus 在主 parser 与 Tracer adapter 上逐 record 运行，oversized buffer 不超过 4 MiB + 常数开销，malformed/EOF 分类与现有 source contract 一致。
- [x] A2/R3：unchanged/append/replace 三态 integration test 证明读取 record 数分别为 0/仅新增/仅受影响文件，最终 rows 与 clean rebuild 等价。
- [x] A3/R4：100k-event synthetic import 的 in-process retained event 数不超过配置 batch；peak RSS 相对基线下降至少 50%，warm wall time 不回归超过 10%。
- [x] A4/R4：跨文件同 thread 的 previous/next/call-index 与一次性全量基线逐字段相等，且重复刷新幂等。
- [x] A5/R5,R6：fresh/upgrade/idempotent/rebuild、mid-batch failure、cancel-and-resume 测试通过，失败后数据库与 cursor 保持同一提交边界。
- [x] A6/R7：旧 public parser/store 路径编译，CLI help、API JSON fixture 和专用 UI smoke 无破坏性差异。
- [x] A7/R8：保存 synthetic before/after；若获得明确授权的 representative copy，再保存其 p50/p95/RSS，否则标记 `UNVERIFIED`。
- [x] A8：focused tracer/main Codex tests、architecture target、fmt、严格 clippy、串行全测试、docs build 和 `just ci` 通过。

## Out of Scope

- 合并 tracer 与主 SQLite schema、移除专用 UI 或重设计 dashboard。
- 改变 token/cost/thread 字段含义。
- 未经 query-plan 证据新增 Tracer 查询索引或引入 rayon/新依赖。

## Key Decisions

- 共享最低 record/envelope primitive，而不是强迫两个消费者共享最终领域模型。
- durable state 使用 byte boundary + file identity；line number 只作为显示/兼容信息，不能作为恢复真值。
- 先流式摄取，再单独基准 query/server；本 child 不扩展到前端性能重写。
