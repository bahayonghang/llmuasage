# 工程审查整改 — 设计（父任务）

## 边界

父任务是协调面。它不拥有 `src/` 变更。每个子任务拥有自己的层：

- 解析失败语义：`src/parsers/antigravity.rs`（child 1）
- 读查询 SQL：`src/query/{activity,tools,home_overview,top_sessions,reports,breakdowns,overview}.rs` 与 `src/tui/data_loader.rs`（child 2）
- SSH argv：`src/remote/transport.rs` + `src/remote/register.rs`（child 3）
- tracer HTTP：`src/commands/codex_tracer/`（child 4）
- loopback 写防护：`src/web/mod.rs`（child 5）
- 分层：pricing/timezone 迁出 `query`（child 6），reports 读路径并入 Dashboard（child 7）
- OpenCode/ZCode 同步（child 8）
- reset/rebuild/定价恢复（child 9）
- 其余安全小项（child 10）
- 日志、文档、死代码（child 11）

## 数据流（审查结论，不是新架构）

```
parsers --SyncShard--> SyncRunWriter.commit_shard --> SQLite
query::Dashboard (one Connection) --> web / tui / export
query::reports (new Connection + ReportFilter) --> CLI daily/weekly/monthly/session/blocks
store currently imports query::pricing and query::timezone  --> cycle
```

目标形状（分两个子任务落地，不在父任务改代码）：

```
domain or store owns pricing + sqlite timezone fns
store does not import query
query::Dashboard is the only read façade; reports take &Connection / QueryFilter extras
```

## 兼容

- 不改变 CLI 标志语义、HTTP 状态码（除非子任务 PRD 写明，如 tracer GET→POST）。
- `--public` 继续无认证、只读聚合。
- 订阅配额拉取继续存在；文档与 architecture「无远程 usage API」对齐到「无用量上传，配额拉取是 TUI 显式行为」。

## 回滚

每个子任务独立提交。父任务归档前若某 child 回滚，映射表把对应发现标回 Still Present。
