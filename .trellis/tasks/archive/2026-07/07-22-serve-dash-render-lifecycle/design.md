# S2 设计：渲染生命周期与自动刷新

## 现状调用链（证据见父 PRD 附录 C26–C36）

- `renderDashboard(rawData)` 全量重建所有面板；展开/折叠、explorer 应用、locale 切换都走它。
- `buildContext` 每次 renderDashboard 至少调 2 次（app.js:196-216），fast-range 流程累计 ~7 次，job 轮询每 900ms 1 次。
- secondary 每到达一个就调 `renderBehavior` 重渲 4 个 section。
- 自动刷新固定 full scope 重取 + 无条件全量重渲。

## 设计决策

### D2.1 渲染注册表与面板指纹

引入轻量 `panelRegistry`：`{ id, render(ctx), fingerprint(ctx) }`。fingerprint 对面板消费的数据子集做稳定序列化（key 排序的 JSON.stringify 或廉价 hash），渲染前与上次值比对，相同则 return。

易变字段剔除清单（指纹计算前剥离）：

- `overview.generated_at`（每查询必变，now_utc）。
- 由 render 层从时间戳派生的相对时间文案（如"x 分钟前"）不进入指纹——指纹只覆盖数据字段，相对时间在数据变化时才更新是可接受的（自动刷新 30s 粒度内相对时间漂移可忽略；若评审要求，可在指纹相同但超过 60s 时仅刷新时间文案节点，列为可选项）。
- secondary section 的 `support`/loading 元数据按现有 stale 语义处理，不剔除。

### D2.2 调用路由

- `state.expanded` 变化 → 只调该面板 render（expand 状态本身是 DOM class 切换的候选，优先纯 class 切换零渲染）。
- explorer 应用 → `renderExplorer`。
- locale 切换 → 复用 D2.3 缓存的 context，直接跑各面板 render（文案必然变化，指纹按 locale 纳入 key：`fingerprint = hash(locale + data)`，locale 变了指纹自然失效，无需特判）。

### D2.3 context 与 formatter 缓存

- `let memo = { raw: null, ctx: null }`：`buildContext(raw)` 当 `raw === memo.raw` 时直接返回 `memo.ctx`（引用相等即可，rawData 不可变更新）。
- `data/format.js` 模块级 `const numberFmt = new Intl.NumberFormat('en-US')` 等，按 locale 缓存小 Map。

### D2.4 behavior section 拆分

`renderBehavior(ctx)` → `renderActivity(ctx)`、`renderTools(ctx)`、`renderOptimize(ctx)`、`renderCompare(ctx)`，各自含自己的 dirty-check。app.js 的 secondary 到达回调（218-225）按 section 名路由到对应函数。`refreshNotice`/stale 元数据逻辑原样保留在各 section 内。

### D2.5 刷新路径

- `scheduleAutoRefresh` 与 sync 完成回调改调既有 `applyInteractiveRange` 流程（app.js:886-942 的 fast-range 路径），不再走 `reloadDashboard` full scope。
- 响应到达 → 剥离易变字段 → 计算指纹 → 与当前渲染态相同则整条渲染链短路（含 secondary 补拉：primary 指纹未变时可跳过 secondary 重取；注意 secondary 数据独立于 range primary 变化的可能——保守方案：primary 未变时仍以并发 2 重取 secondary 但各自 dirty-check，渲染零写入）。
- 契约不变：请求/响应字段零改动，指纹纯客户端计算。

### D2.6 job 轮询

- 轮询快照先与上次快照做浅字段比对，只把变化字段写入对应文本节点（进度、状态行）；面板结构不变时不触碰 innerHTML。
- 终态（success/failed/cancelled）到达后停止轮询；加一个总时长上限（如 30 分钟）防死循环。
- click 绑定改为容器一次委托。

## 测试策略

- node 测试：指纹剔除逻辑、context memo 命中、locale 指纹失效、behavior 拆分后的 section 独立性。
- 插桩计数（测试注入）：buildContext 调用数、NumberFormat 构造数、面板 render 调用数。
- MutationObserver 范围断言（fixture + 现有 JS 测试设施，如不足则在 node 测试中以最小 DOM stub 覆盖关键路径）。
- 性能证据：`scripts/benchmark-dashboard-range.mjs --url <representative-copy> --iterations 5`。

## 风险

- 指纹漏剔易变字段 → 表现为"缓存永不命中"，无损正确性；测试覆盖剔除清单。
- rawData 被原地 mutate 会导致 memo 失效漏判 → 约定 rawData 不可变，替换式更新（现有代码已是 `{...snapshot}` 风格，复核一遍）。
