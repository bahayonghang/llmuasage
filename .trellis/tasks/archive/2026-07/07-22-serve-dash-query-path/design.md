# S3 设计：diagnostics 缓存与 full scope 组合器

## 现状（证据见父 PRD 附录 D39–D45）

- `Dashboard::diagnostics()` 对 `source_file` 全表逐行 `Path::exists()`（src/query/mod.rs:2736-2759），被 core/interactive snapshot 与 `/api/diagnostics` 调用，无缓存。
- full scope：先 await core（1 permit），再 `tokio::join!` 5 个 secondary（src/web/mod.rs:952-997）；并发需求 5、`WEB_DASHBOARD_QUERY_PERMITS=4`（mod.rs:42）→ 第 5 个排队；信号量等待计入 5s 预算，第二标签页并发可能级联超时。
- behavior 四 section（activity/tools/optimize/compare）有 1s 超时 + `degraded_*` 降级（mod.rs:41,902-930）；explorer 无降级包装，失败会使 full 请求整体失败（mod.rs:1001 `explorer?`）——现状语义，保留或改善需在实施时明确记录。
- `Dashboard::snapshot()`（query/mod.rs:2332）供静态导出：单连接、顺序、`?` 传播任一错误——不满足 live 降级需求。

## D3.1 WebState 级 diagnostics 缓存

```
struct DiagnosticsCache { value: DiagnosticsPayload, computed_at: Instant }
WebState.diagnostics_cache: RwLock<Option<DiagnosticsCache>>  // TTL 30–60s
```

- 读取路径（core/interactive/`/api/diagnostics` handler 层）：命中且未过期 → 克隆返回；过期/缺失 → `spawn_blocking` 计算后写入。**缓存在 web handler 层，不进 `Dashboard`**。
- 失效：sync job 到达终态时主动清空（job 轮询在服务端 JobRegistry 已有状态，web 层在 job 完成回调点清）；TTL 兜底外部文件删除。
- 并发去重：过期时多请求只让一个计算（single-flight，用既有 inflight 模式或 tokio OnceCell/Notify）。
- TTL 值在实施时用基线数据定（stat 成本 vs 数据新鲜度），写入代码注释与本文件。

关键不变量：`Dashboard::diagnostics()` 与 `home_overview` 零改动——80ms cold 测试是直接证据。

## D3.2 live 专用 full 组合器

目标形状：

```
full = core(同一连接) → 顺序执行 5 个 secondary(同一 Dashboard/连接)
       每个 behavior section 包 1s 超时 + degraded 兜底（等效 load_behavior_api）
       explorer 保持现状语义（失败整体失败）或显式降级（实施时记录选择）
```

- permit 占用恒为 1（一个 spawn_blocking 任务持有），并发 4 个 full 请求不再互相排队。
- 与现状的差异是顺序化：secondary 串行后单请求墙钟可能变长——用基线数据评估；若 secondary 总耗时 > 现状并行墙钟的不可接受阈值（实施时定为 full p95 不退化超过 20%），备选方案是 core 单连接 + secondary 仍并发但共享一个连接池条目不可行（rusqlite Connection 非 Sync），则退而保持并发结构、仅修 R3.2/R3.4/R3.5 并将 R3.3 结论记录为"不实施"。这是本任务"先测量后方案"的核心原因。
- 替代设计（若顺序化不可接受）：permit 数提升 + full 请求自限并发（如 secondary 并发 2），同样需测量支撑。

## D3.3 compare N+1

一条 `SELECT model, COUNT(*), SUM(has_edits) FROM usage_turn WHERE model IN (...) GROUP BY model`（或 join 候选表），内存按 model 匹配；空候选短路。等价测试覆盖：无候选、候选无 turn、25 候选上界。

## D3.4 busy_timeout

web 读连接单独设 `busy_timeout(1–2s)`。检查 `Store::open_connection` 的调用方，确保只影响 web/API 读路径；sync 写连接保持 30s。

## 测试策略

- stress fixture：seed 数千 source_file + 25 model（扩展 `Fixture` 或测试内构造），断言 stat 计数（用包装层计数或 tracing）。
- 缓存失效矩阵：TTL 内命中 / TTL 过期重算 / sync 完成失效 / single-flight 并发去重。
- 降级回归：人为注入 section 故障（既有 degraded 测试模式），断言其余 section 不受影响。
- 双并发 full：测试内起两个并发请求，断言均成功且耗时 < 5s。
- cold 契约：80ms 测试 debug/release 各连跑 3 次。
- 等价：compare 前后输出逐字段对比（序列化 JSON diff）。

## 风险

- 缓存一致性窗口内用户删掉归档文件但 banner 未更新——TTL ≤60s 可接受，且 sync 失效覆盖主要路径；在 docs 注明。
- 顺序化 wall-clock 退化——基线先行，不达标则按 D3.2 备选或"不实施"记录。

## 最终决策（2026-07-22）

**不实施 D3.2 的单连接顺序 full 组合器，保留现有并行逐 section 降级结构。**

- 代表库基线中 5 个 secondary 顺序总耗时约 7.6s；即使 activity/tools/optimize 各按 1s 截断，预计仍约 3.5s。现有 full 单请求约 1.19s，顺序方案远超“p95 不退化超过 20%”门槛。
- `Dashboard::snapshot()` 仍不适合 live 路径：它缺少逐 section 的 1s 超时与 degraded payload，不能作为替代。
- 本任务最终实施 R3.2 diagnostics WebState 缓存、R3.4 compare candidates 单条 GROUP BY、R3.5 web 读连接 1500ms busy timeout；这些收益不以牺牲 live 降级契约为代价。
- 最终代表库复测中 full 请求均在约 1.21–1.52s 返回 HTTP 200，explorer 保持 normalized；activity/tools/optimize 因查询本身超过 1s 而 degraded。compare candidates 已消除 N+1，但完整 `model_compare` 在重查询并发下仍可能超过 1s 并 degraded。该 residual 明确记录，不将其误报为 R3.3 已解决。
