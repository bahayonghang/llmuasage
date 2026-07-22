# Implement（S2 渲染生命周期与自动刷新）

前置：prd.md（需求/验收）与 design.md（技术方案 D2.1–D2.6）已审定。纯前端任务，不改 Rust。

## Checklist

- [x] `data/format.js`：`Intl.NumberFormat` 模块级缓存（按 locale 小 Map），导出 API 不变。
- [x] `data/derive.js`：`buildContext(raw)` 引用缓存（`raw === memo.raw` 命中）；确认 rawData 全链路不可变替换式更新。
- [x] 新增面板指纹机制（建议放在新纯模块如 `data/fingerprint.js`，便于 node 测试）：稳定序列化 + 易变字段剔除清单（`overview.generated_at` 等，见 design.md D2.1）；指纹 key 含 locale。
- [x] 渲染注册表/逐面板 dirty-check：hero/trends/models/sources/projects/costs/behavior/explorer/sync-command-center 各渲染入口先比对指纹，未变跳过 DOM 写入。
- [x] 调用路由：面板展开/折叠只重渲对应面板（优先纯 class 切换）；explorer 应用只调 renderExplorer；locale 切换复用 context 缓存重渲。
- [x] renderBehavior 拆分为 renderActivity/renderTools/renderOptimize/renderCompare，各自 dirty-check；app.js secondary 回调按 section 路由；保留 stale/refreshNotice 语义。
- [x] 自动刷新（app.js:962-973）与 sync 完成重载（app.js:1473-1475）改走既有 interactive 路径；响应指纹未变则整条渲染链短路（含 secondary 策略按 design.md D2.5 保守方案）。
- [x] job 轮询：快照浅比对只更新变化节点；终态停止 + 总时长上限；sync-command-center 改容器级事件委托。
- [x] 测试：scripts/tests/ 新增测试（指纹剔除、memo 命中、locale 指纹失效、formatter 复用、behavior 拆分独立性）；既有 dashboard-fetch.test.mjs 保持绿。
- [x] 测量：fixture 下插桩计数对比（buildContext 次数、DOM 写入），记入任务目录 research/。

## Suggested Implementation Order

1. 基础层：formatter 缓存 + buildContext memo + 指纹纯模块（含单测）。
2. 渲染层：注册表 + 逐面板 dirty-check + 调用路由。
3. behavior 拆分 + secondary 路由。
4. 刷新路径切换（interactive + 指纹短路）。
5. job 轮询瘦身。
6. 插桩测量 + 文档化结果。

## Validation Commands

```bash
node --check src/web/assets/app.js  # 及每个改动文件
node --test scripts/tests/
cargo run --features testing --example docs_dashboard_serve -- --port 37426 --timeout-secs 60
node scripts/benchmark-dashboard-range.mjs --url http://127.0.0.1:37426 --iterations 5
```

## 边界

- 不改 API 契约字段；不引入依赖；保持既有 generation/abort 语义（dashboard-performance-contracts）。
- 行尾：app.js 等文件含 CRLF 混合，编辑保留原行尾，不整文件转换。
