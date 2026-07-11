# 实施计划 - Cost Explorer 时间序列紧凑展示

## Gate 0 - 开始实施前

- [x] 用户审阅并批准 `prd.md`、`design.md` 和本计划。
- [x] 批准后运行 `python ./.trellis/scripts/task.py start 07-11-explorer-time-series-layout`；
      在此之前不改生产代码。
- [x] 加载 `trellis-before-dev`，重新读取相关项目规范、`DESIGN.md` 与 Web 资产约定。
- [x] 检查工作树基线并保留无关改动。当前已知 `Cargo.toml` 的 `0.9.1` 版本改动不属于本任务，
      不得覆盖或纳入本任务修改。

## Step 1 - Shell 结构与文案契约

- [x] 在 `src/web/shell.rs` 中将趋势 chart host 留在两栏结果区，并把 details host 移到
      `.explorer-results-grid` 之后，使明细横跨完整宽度。
- [x] 保留现有 Explorer controls、summary、warning、排行 DOM id 和查询绑定，不改 API 行为。
- [x] 在 `copy.js` 增加 chart scope、独立刻度、峰值、明细、时间范围和截断说明的中英文 key。
- [x] 为 `total`、无数据和降级状态保留可读 fallback。

验证：

```powershell
cargo test --lib web::tests::live_shell_uses_data_i18n_for_chrome -- --exact
```

回滚点：shell host 和 copy key 可独立恢复，不触及 renderer 数据逻辑。

## Step 2 - 趋势展示模型与 SVG 小多图

- [x] 在 `render/explorer.js` 中提取有界常量：最多 5 个 chart series、最多 80 行明细。
- [x] 从 `rows` 按总量选择在 `series` 中出现的前 5 个 key，保持 `Other` 的普通排序语义。
- [x] 构建排序后的共享 bucket 集合和逐 key value map，对已有 bucket 的缺失维度补零。
- [x] 为每个维度计算独立最大值、峰值和稳定 SVG 坐标；覆盖零值、单 bucket 和多 bucket。
- [x] 渲染最多 5 行带 label、独立尺度 SVG 与 peak value 的 small multiples；图表使用完整
      series 响应，不受表格 80 行上限影响。
- [x] 图表范围文案明确“前 5 个维度”和“各维度独立刻度”，未展示维度指向明细。
- [x] 保持 renderer 无 `fetch()`、无原始事件透视、无后端契约复制。

验证：

```powershell
cargo test --lib web::tests::dashboard_assets_wire_explorer_workbench_without_frontend_pivoting -- --exact
```

回滚点：若 chart model 无法稳定处理所有 metric/dimension，停止进入样式步骤并恢复表格 renderer。

## Step 3 - 折叠明细与高度约束

- [x] 使用原生 `<details>` 渲染明细，默认关闭，并在普通 rerender 中保留当前 open 状态。
- [x] summary 显示总点数和完整 bucket 范围。
- [x] `series.length <= 80` 时展示全部；超过 80 时展示最近 80 条和明确的“80 / N”提示。
- [x] 表格外层限高为 `min(420px, 50vh)` 并内部滚动；thead sticky，数值继续右对齐。
- [x] `granularity=total`、no data 和 warning 状态不生成空 details 或误导性范围。

验证：

```powershell
cargo test --lib web::tests::dashboard_assets_wire_explorer_workbench_without_frontend_pivoting -- --exact
```

## Step 4 - 主题与响应式样式

- [x] 在 `components.css` 建立 Explorer trend panel、small-multiple row、details summary、scroll host
      的稳定尺寸和网格。
- [x] 在 `charts.css` 增加使用 `--data-accent` / instrument tokens 的 path、point、grid 和 axis
      样式，不引入按维度分配的多色 palette。
- [x] 宽屏保持排行/趋势两栏且高度视觉平衡；`720px` 以下切为单列并压缩 label/value tracks。
- [x] summary 触控高度至少 44px；超长 label 截断；表格和 SVG 不产生水平溢出。
- [x] 不添加布局属性动画；focus、hover 和 reduced-motion 状态保持一致。

验证：

```powershell
cargo test --lib web::tests::dashboard_assets_style_sync_command_center_responsively -- --exact
```

## Step 5 - 资产契约测试

- [x] 扩展 `src/web/mod.rs` 的 Explorer 测试，断言 chart/details host、前 5/80 上限、完整 bucket
      chart 路径、截断提示和相关 CSS class/copy key 存在。
- [x] 断言 renderer 仍不包含 `fetch(`，API/snapshot 加载路径不变。
- [x] 增加 total/no-data、单 bucket、全零和超过 80 行所需的静态契约或最小可测 helper 断言；
      不写只能证明字符串存在的重复测试。

验证：

```powershell
cargo test --lib web::tests::dashboard_assets_wire_explorer_workbench_without_frontend_pivoting -- --exact
cargo test --lib web::tests::live_shell_uses_data_i18n_for_chrome -- --exact
```

## Step 6 - 浏览器视觉与交互验收

- [x] 启动本地 dashboard，使用真实或隔离 fixture 生成至少 60 个 series points。
- [x] 在 1440x900 与 390x844 检查默认收起高度、两栏/单列结构、无重叠和无水平溢出。
- [x] 分别检查中文/英文、浅色/深色，共四种语言主题组合。
- [x] 展开明细并验证内部滚动、sticky header、summary 键盘操作与收起后的页面高度恢复。
- [x] 修改 Top N、granularity、metric 和 group-by 后运行分析，确认图表/范围/明细同步更新。
- [x] 检查 snapshot/export 页面复用同一行为；截取桌面和移动端验收截图。

## Step 7 - 最终质量门

- [x] `cargo fmt --check`
- [x] `cargo test --lib web::tests::dashboard_assets_wire_explorer_workbench_without_frontend_pivoting -- --exact`
- [x] `cargo test --lib web::tests::dashboard_assets_style_sync_command_center_responsively -- --exact`
- [x] `cargo test -- --test-threads=1`
- [x] `git diff --check`
- [x] 运行 `trellis-check`，修复发现并重复受影响 gate。
- [x] 仅在 Web 行为文档与新交互不一致时更新中英文 dashboard docs；纯布局实现不制造无关文档改动。
- [x] 完成 spec 同步判断：无命令/API/数据库/跨层契约变化；现有 `DESIGN.md` 已覆盖可复用
      的前端规范，任务级 chart/details 契约保留在本任务设计中，不新建 llmusage frontend spec 层。

## Risk Files

- `src/web/assets/render/explorer.js`：chart model、缺失 bucket、格式化和 details 状态集中处。
- `src/web/assets/components.css` / `charts.css`：固定高度、sticky/overflow 与移动端布局。
- `src/web/shell.rs`：live 与 snapshot/export 共用的 DOM contract。
- `src/web/mod.rs`：静态资产契约测试可能对精确字符串和换行敏感。

## Rollback Strategy

本任务没有数据库或 API 迁移。若 small multiples 在真实数据上无法保持可读，可恢复旧
`#explorer-series` host 和 `renderSeriesTable()`，同时撤回对应 CSS/copy/test；后端查询、快照文件
和用户数据无需回滚。不得通过删除明细、静默截断或改变 Explorer 查询语义来规避视觉问题。
