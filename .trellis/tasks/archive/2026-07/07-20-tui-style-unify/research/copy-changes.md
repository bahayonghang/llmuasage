# TUI copy changes

Stage two standardizes the interactive TUI on English. Repeated loading and
error strings are listed once with all affected panels.

| Surface | Old | New |
| --- | --- | --- |
| Overview, Models, Sources, Projects, Cost, Health, Behavior, Trends, Blocks | `加载中...` | `Loading...` |
| Same panels | `数据加载失败: {e}` | `Data load failed: {e}` |
| Overview | `概览` | `Overview` |
| Overview | `累计 Tokens` | `Total Tokens` |
| Overview | `累计成本` | `Total Cost` |
| Overview | `缓存命中率` | `Cache Hit Rate` |
| Overview | `从未同步` | `Never synced` |
| Models | `模型` | `Models` / `Model` |
| Models | `暂无模型数据` | `No model data found.` |
| Sources | `来源` | `Sources` / `Source` |
| Sources | `暂无来源数据` | `No source data found.` |
| Sources | `最近事件` | `Last Event` |
| Projects | `项目` | `Projects` / `Project` |
| Projects | `暂无项目数据` | `No project data found.` |
| Cost | `成本` | `Cost` |
| Cost | `暂无成本数据` | `No cost data found.` |
| Cost | `估算成本` | `Estimated Cost` |
| Shared table headers | `总 Tokens` | `Total Tokens` |
| Shared table headers | `事件数` | `Events` |
| Shared table headers | `成本 (USD)` | `Cost (USD)` |
| Blocks | `区块` | `Blocks` |
| Blocks | `暂无区块数据` | `No block data found.` |
| Blocks | `窗口` | `Window` |
| Blocks | `状态` | `Status` |
| Blocks | `预计` | `Projected` |
| Blocks | `额度` | `Limit` |
| Blocks | `区块 (5h burn rate)` | `Blocks (5h burn rate)` |
| Health | `健康` | `Health` |
| Health | `无数据` | `No data` |
| Health | `集成状态` | `Integration Status` |
| Health | `未更新` | `Never updated` |
| Health | `游标` | `Cursors` |
| Health | `无失败记录` | `No failure records` |
| Health | `近期失败` | `Recent Failures` |
| Trends | `趋势` | `Usage Trends` |
| Trends | `暂无趋势数据` | `No trend data found.` |
| Trends | `窗口:` | `Window:` |
| Trends | `h/l 或 ←/→ 切换` | `h/l or left/right to switch` |
| Trends | `总量` | `Total` |
| Trends | `峰值` | `Peak` |
| Trends | `桶均` | `Avg / bucket` |
| Trends | `日均` | `Daily avg` |
| Trends | `活跃桶` | `Active buckets` |
| Trends | `活跃月` | `Active months` |
| Trends | `活跃天` | `Active days` |
| Trends | `共 {n}` | `{n} total` |
| Trends | `趋势图` | `Trend Chart` |
| Trends | `最近` | `Recent` |
| Trends | `最近明细` | `Recent Details` |
| Behavior | `行为` | `Behavior` |
| Behavior | `无 Activity 行为事实。` | `No activity behavior facts.` |
| Behavior | `行为 · Activity 分类` | `Behavior / Activity Categories` |
| Behavior | `无 Tools 行为事实。` | `No tool behavior facts.` |
| Behavior | `行为 · Tools 工具` | `Behavior / Tools` |
| Behavior | `只读建议：llmusage 不会自动删除、归档、重写或清理任何内容。` | `Read-only advice: llmusage never deletes, archives, rewrites, or cleans content automatically.` |
| Behavior | `无行为事实，暂不计算 score 或 savings。` | `No behavior facts; score and savings are not calculated.` |
| Behavior | `未发现明显浪费模式；继续结合上下文人工判断。` | `No obvious waste patterns found; review the surrounding context manually.` |
| Behavior | `{evidence}；建议：{recommendation}` | `{evidence}. Recommendation: {recommendation}` |
| Behavior | `僵尸技能/MCP：无（已扫描 {n} 个已装项）` | `Zombie skills/MCPs: none ({n} installed items scanned)` |
| Behavior | `僵尸 {n}` | `Zombies {n}` |
| Behavior | `装了从未调用...` | `Installed but never called...` |
| Behavior | `行为 · Optimize 只读建议` | `Behavior / Optimize (read-only)` |
| Behavior | `警告` | `Warning` |
| Behavior | `Compare 需要至少两个有本地用量的模型。` | `Compare requires at least two models with local usage.` |
| Behavior | `候选模型: {n}` | `Candidate models: {n}` |
| Behavior | `行为 · Compare 模型对比` | `Behavior / Model Comparison` |
| Behavior | `状态` | `Status` |
| Footer | `tab/1-8` | `tab/1-9` |
| Footer | `tab/shift-tab or 1-8 view` | `tab/shift-tab or 1-9 view` |
