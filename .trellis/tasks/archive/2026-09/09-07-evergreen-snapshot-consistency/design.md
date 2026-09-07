# 保证看板数据库快照一致性：设计

## Mechanism and tradeoffs

在复合 snapshot 入口使用短SQLite deferred read transaction，第一次数据库读取建立快照；所有数据库指标子查询复用该连接。采用一个私有with_read_snapshot边界：conn.is_autocommit为true时由当前最外层调用开启并以RAII结束读事务，已有内部快照事务则复用，不嵌套BEGIN；不要在Dashboard::open时开启跨多个独立请求的长事务。snapshot→core_snapshot及interactive/core的with_diagnostics入口均经过该边界，诊断缓存/文件扫描在进入数据库指标事务前获取；独立health/diagnostics不声称同一数据库版本。数据库一致性测试通过内部测试hook/barrier同步，不靠sleep竞态。外部诊断保留现有降级/独立采样语义，不新增一整套一致性状态。

这是静态证据支持的P2设计，实施先补红色并发回归；若实际现有入口已有外层事务保护，撤回无效修改并记录证据。

## File ownership

- `src/query/snapshot.rs`
- `src/query/mod.rs`
- `src/query/tests/diagnostics_snapshot.rs`
- `src/query/tests/mod.rs`
- `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`

## Tool and model assignment

Codex或Claude Code强模型决定SQLite事务范围及timeout语义；事务实现与barrier测试可分给便宜模型，最终并发不变量由强模型复审。

## Failure and rollback

单项提交前用diff保留无关改动；失败只撤销本任务补丁，不重置用户工作树。不存在自动发布、自动升级远程或全局设置的授权。门禁失败需定位具体操作；未执行的原生/远程验证保留UNVERIFIED。

## Documentation writeback

批准并通过验收后同步上述拥有的项目说明/spec；适用工具标注为 Claude Code、Codex、Grok Build、Kimi Code、OMP。跨工具公共说明由 harness-contracts 子任务最终汇总。任务计划本身不是已生效规则。
