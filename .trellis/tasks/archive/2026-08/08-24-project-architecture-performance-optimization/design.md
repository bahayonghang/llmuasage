# Design

## Architecture Strategy

父任务采用“深模块 + 薄兼容层”策略，不建立新的通用 repository/service 框架：

```text
Codex rollout bytes
  -> shared bounded record/envelope boundary
     -> main UsageEvent sink
     -> CodexTracerEvent sink -> independent tracer DB/UI

CLI/Web/TUI adapters
  -> sync application engine -> parser driver -> SyncRunWriter

Dashboard facade
  -> vertical query modules -> one SQLite connection / existing read models

Report CLI
  -> one period aggregate pass -> all/source/host projections
```

兼容层只能 re-export、构造或格式化；核心状态机、SQL、重建策略和聚合规则必须由所属层拥有。

## Child Boundaries

### 1. Codex Tracer ingestion

共享“读取完整有界 JSONL record + 解码 Codex envelope”的最低公共层。主 parser 和 tracer 各自拥有领域投影；tracer 继续拥有独立 schema/store/UI。不得让 tracer 依赖主 `UsageEvent` 才能表达其 44 字段调用模型。

### 2. Sync application boundary

`sync` 拥有执行接口、默认 executor、重建/修复/远端导入编排和结果；`commands::sync` 只拥有参数到 typed request、human/NDJSON progress、摘要打印和 Ctrl-C 适配。旧 `commands::sync::CommandSyncExecutor` 可作为兼容 re-export，但不再拥有实现。

### 3. Query modularization

保持 `Dashboard` 为单连接 facade。按 overview/trends/breakdowns/behavior/compare/diagnostics-synchronization/snapshot 切垂直模块；现有 explorer、home_overview、logs、top_sessions、heatmap、hour_of_week 不重写。移动类型时从 `query` 和 crate root 重新导出原路径。

### 4. Reports

用一个 period aggregate core 产生 overall/source/host 投影，PeriodSpec 只负责 period key 和 DTO 映射。项目模糊值先解析为匹配的 project hashes，再走 bucket/fact 精确过滤；只有无法由 projection 精确表达的语义才保留 event fallback。

## Compatibility

- Rust：保留 `llmusage::{Dashboard, JobRegistry, ...}` 与已公开 compatibility module 路径。
- CLI：保留参数、exit code、stdout/stderr 分工、JSON key/casing/order。
- SQLite：tracer 与主库继续独立；新增 tracer cursor/schema 必须前向迁移、幂等且可 rebuild。
- Web/TUI：序列化字段、degraded/cancelled 语义和既有 performance budgets 不变。

## Performance Evidence Model

每个性能结论标记数据级别：

- `synthetic`：固定 seed，可进测试；用于等价、statement count、复杂度和稳定 budget。
- `representative-copy`：只读源的临时备份；用于 p50/p95、RSS、迁移时间。
- `historical`：已有 task evidence；仅用于选择候选，不作为最终验收。
- `UNVERIFIED`：缺少真实浏览器/冷缓存/RSS 等证据，不得写成 PASS。

## Rollback

每个 child 独立 commit。失败时按 child 回滚，不回退其他 child：compat re-export → module move → engine/query change → schema/index 最后。父任务不做 squash 式跨域回滚。

