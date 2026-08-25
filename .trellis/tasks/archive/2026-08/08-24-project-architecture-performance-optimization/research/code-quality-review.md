## Verdict

COMMENT ONLY

当前没有待合并产品 diff，但项目存在四项高置信度结构/性能债务；应按独立子任务处理，避免一次性全库重构。

## Findings

### CQ-001 [High] Codex Tracer 复制了解码/存储/Web 栈且摄取内存无界

- Location: `src/commands/codex_tracer/`；`mod.rs:68-86`；`parser.rs:61-110`
- Evidence: 独立子系统约 8,415 行；每个文件返回 `Vec<CodexTracerEvent>`，命令层再累积 `all_events`；parser 使用 `BufRead::lines()`，final state 的行号取最后一个 usage event，而不是最后 durable record。主 Codex parser 已在 `src/parsers/codex.rs:560-646` 使用有界 reader。
- Why it matters: 峰值内存随全部历史事件增长，两套 envelope 语义、EOF/oversized/cancellation 规则会漂移；所谓 incremental state 仍需跳过先前行，并会重读最后 usage 之后的非 usage 记录。
- Recommended remediation: 保留独立数据库/UI，但抽取共享 bounded record/envelope primitive；tracer 按有界 batch 事务写入并持久化 durable byte/file identity state。
- Confidence: High

### CQ-002 [High] 稳定同步 API 的默认执行与核心编排由 CLI 命令层拥有

- Location: `src/commands/sync.rs:535-855`；`src/sync/job_registry.rs:83-101`；`src/lib.rs:87-93`
- Evidence: crate root 稳定导出 `JobRegistry`，但 `impl Default for JobRegistry`、`CommandSyncExecutor` 和 `run_once_locked_with_remote_source` 均位于 `commands::sync`；Web/TUI 直接构造 `commands::sync::CommandSyncExecutor`。
- Why it matters: application API 的默认行为依赖被文档标为 compatibility/CLI internal 的模块，重建、修复、远端导入和输出适配难以独立演化或嵌入。
- Recommended remediation: 将 engine/default executor 迁入 `sync`，commands 只处理 transport 与呈现；旧公共路径只 re-export。
- Confidence: High

### CQ-003 [High] `query/mod.rs` 是多个查询子域的 canonical owner

- Location: `src/query/mod.rs:50-4198`
- Evidence: 测试区从 4,199 行开始；生产区同时定义大量 DTO 和 `Dashboard` 的 overview、trends、activity、tools、optimize、compare、health、sync center、snapshot 等方法。explorer/home_overview/logs/top_sessions 等已证明垂直模块边界可行。
- Why it matters: 任一子域修改都增加中心文件冲突和上下文负担，私有 helper/DTO 所有权难以辨认，性能修复容易跨域复制 filter/SQL。
- Recommended remediation: 保持 `Dashboard` 单连接 facade 和 re-export 兼容，按垂直 feature 拆模块并用架构 fixture 固化依赖方向。
- Confidence: High

### CQ-004 [High] 统一周期报表重复聚合，模糊项目过滤退化为逐事件对象扫描

- Location: `src/query/reports.rs:548-1112,1266-1337,2021-2052,2288-2301`
- Evidence: 日/周/月与 source/host 的构建逻辑平行；`load_unified_report` 先加载 overall，再加载 by-source，导致相同 bucket/event 范围二次读取。项目过滤只下推 date/source/host，随后为每条 event 构造对象并在 Rust 中模糊匹配。100k backlog 合成回归通过但总测试 wall time 为 4.23 s；该 wall time包含 fixture 写入，不能当作纯查询耗时。
- Why it matters: 分支数量和 DTO 映射容易漂移；全历史 `--project` 的 CPU、分配与 I/O 随事件数线性增长，并在 unified 路径重复。
- Recommended remediation: 一次 period aggregate 同时产出 overall/source/host；先通过 `project_dim`/bucket labels 解析 project hashes，再使用精确 hash + bucket fast path，保留必要 event fallback。
- Confidence: High

### CQ-005 [Medium] 架构测试只保护两个单向禁用边

- Location: `tests/architecture/main.rs:308-337`
- Evidence: 当前测试仅断言 `sync` 与 `remote` 不依赖 `commands`；它不表达 commands 不应拥有 sync 类型实现、query/store/parser 层图或 compatibility re-export 约束。3/3 测试通过但 CQ-002 仍存在。
- Why it matters: 局部绿色无法证明 canonical ownership，后续可通过反向 impl、type alias 或别的模块再次形成环状所有权。
- Recommended remediation: 把允许的层图与兼容 re-export 规则编码为 AST fixture，至少覆盖 sync、remote、query、store、parsers 和 commands。
- Confidence: High

## Checked but not flagged

- `src/web/mod.rs` 总行数很大，但生产区约 77 行，主要体量来自同文件测试，未按裸行数列为产品 god-module。
- `src/store/migrations.rs` 与 `src/store/sync_writer.rs` 的大部分额外体量来自高价值迁移/事务测试；没有仅凭文件大小提出拆分。
- 既有历史证据显示 interactive dashboard 与 Top Sessions 的代表性预算已达标，因此未提出无差别索引或并发扩容。
- Store facade、SyncShard commit 和独立 tracer database 均有既有 ADR/spec 决策，本方案保持这些边界。

## Scope limitations

- 本轮没有读取用户活动数据库，也没有进行 release RSS、冷文件缓存、真实浏览器或真实 Codex rollout 基准。
- 4.23 s 是合成测试总 wall time，不是隔离 SQL timing；真正 before/after 必须由子任务 harness 捕获。
- 历史代表性数据库指标只用于候选排序，可能随当前数据/机器/HEAD 漂移，最终验收必须刷新。

