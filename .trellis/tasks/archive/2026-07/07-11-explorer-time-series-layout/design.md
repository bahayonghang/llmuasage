# Cost Explorer 时间序列紧凑展示设计

## 1. Design Summary

本任务只调整 Web 展示层。Explorer 继续消费后端已经聚合的 `rows` 和 `series`，
查询参数、SQL、Rust payload 和 snapshot/export 数据契约全部保持不变。

```text
ExplorerPayload.rows + ExplorerPayload.series
                    -> 前端展示模型
                    -> 左：维度排行
                    -> 右：最多 5 行趋势小图
                    -> 下：默认折叠的时间序列明细
```

趋势小图用于读取形态，维度排行用于比较绝对量级，明细表用于读取精确值。三者职责
分开后，默认页面高度不再由“时间桶 x 维度数”决定。

## 2. Scope And Boundaries

### In Scope

- 重排 Explorer 结果 DOM，使排行与紧凑趋势并排，明细横跨结果区并默认收起。
- 从聚合 `rows` 中选择总量最高且有 series 数据的最多 5 个维度。
- 从聚合 `series` 构建共享时间桶、逐维度独立尺度的 SVG 小多图。
- 为明细增加点数、时间范围、截断状态、限高滚动和 sticky 表头。
- 增加中英文文案、响应式样式、主题样式、资产契约测试和浏览器截图验证。

### Out Of Scope

- `src/query/explorer.rs`、`/api/explorer`、query string 和 snapshot schema。
- 第三方图表库、服务端分页、虚拟列表和新的用户持久化设置。
- Explorer 筛选器、排行算法、Top N/Other 业务语义的重做。

## 3. Layout And DOM

`src/web/shell.rs` 把当前单个 `#explorer-series` host 拆成明确的图表与明细 host：

```text
.explorer-results-grid
  .explorer-ranking
    #explorer-bars
    #explorer-rows
  .explorer-trends
    title + subtitle
    #explorer-series-chart

#explorer-series-details
  details (closed by default)
    summary: 明细标题 + 数据点/时间范围
    truncation notice (conditional)
    scroll container
      table with sticky thead
```

宽屏继续使用两栏，但两栏只承载有界内容；明细移到 grid 之后并横跨整行。`720px`
以下沿用现有响应式断点，将排行、趋势和明细按阅读顺序堆叠。

## 4. Chart Presentation Model

`render/explorer.js` 只转换已经聚合的 payload，不读取或透视原始 usage events。

1. 校验 `rows` / `series` 为数组，并按 `rows.value` 降序得到排行。
2. 只保留在 `series` 中实际出现的 key，取前 5 个；`Other` 与普通维度相同，按总量
   决定是否进入前 5。
3. 对全部 `series.bucket` 去重并按后端可排序的 bucket key 升序排列，作为共享 X 轴。
4. 对每个入选维度建立 `bucket -> value` 映射。共享桶中缺失该维度时填零；这只补齐
   聚合矩阵中的缺项，不改变后端查询或总量。
5. 每个维度以自身最大值为纵轴上限；最大值为零时使用稳定的零基线。
6. 小图覆盖全部 bucket。X 轴只渲染少量自适应标签，避免标签数量随数据增长。

独立尺度会在趋势区说明中明确标注；每行同时显示维度名和格式化峰值。绝对量级仍由
左侧排行的条形长度、数值和占比表达，避免用户拿不同小图的高度直接比较金额或调用量。

## 5. SVG Rendering

- 不引入依赖，复用项目现有 inline SVG 方式和 `charts.css`。
- 每行是稳定高度的小图，包含 label、SVG plot 和 peak value；最多 5 行，因此趋势区高度
  保持有界。
- 一个 bucket 渲染单点；两个及以上 bucket 渲染 polyline/path。零值保留在基线。
- 线、峰值点和 hover/focus 状态使用现有 `--data-accent` / instrument tokens，不为维度
  引入新的品牌色。
- 每行提供可读 `aria-label`，包含维度、时间范围和峰值；精确逐点值由后续明细表提供。
- 不添加入场动画，避免刷新和 `prefers-reduced-motion` 产生额外分支。

## 6. Details Behavior

- 使用原生 `<details>` / `<summary>`，默认收起并继承项目已有键盘行为和 focus ring。
- summary 显示总 series point 数和完整起止 bucket；没有时间点时走现有 empty state。
- 保留当前 80 行 DOM 上限，但改为按后端顺序取最近 80 条，避免长区间时只显示最旧数据。
- 当 `series.length > 80` 时显示“已显示 80 / 共 N 条”及“趋势图仍覆盖完整区间”。
- 表格外层使用 `max-height: min(420px, 50vh)` 与 `overflow: auto`；thead 在滚动容器内
  sticky。
- 常规刷新时尽量保留用户当前展开状态；首次渲染和新页面加载保持收起。

## 7. States And Edge Cases

- `granularity=total`：不绘制趋势，显示现有“总计不返回时间序列”状态，明细不伪造数据。
- `no_data` / `unsupported` / `degraded`：继续由 support/warning 表达；图表和明细使用同一
  reason，不隐藏异常。
- 只有一个 bucket：用点而不是零长度折线。
- 某维度全零：显示稳定基线和 `$0`/`0` 峰值，不产生 `NaN` 坐标。
- 相同 label、不同 key：内部始终以 key 关联，label 只用于展示。
- 超长 session/project/tool label：单行截断并提供 `title`，不得撑宽图表列。
- 未进入前 5 的维度：趋势区明确提示其仍在明细中，不能暗示为零。

## 8. I18n, Theme And Responsive Rules

- 新增 chart scope、independent scale、peak、detail summary、range 和 truncation 的中英文
  `copy.js` key；渲染器通过 `getShellCopy()` 获取文案。
- 图表只消费 `base.css` 的语义 tokens；浅色/深色主题不维护重复硬编码色值。
- 桌面图表高度目标约 280-340px；移动端 label/plot/value 改为更紧凑网格，SVG 宽度随容器
  变化，不使用 viewport 字号缩放。
- details summary 触控目标至少 44px；键盘 focus 必须清晰。

## 9. Files And Responsibilities

- `src/web/shell.rs`：结果区语义结构与 chart/details host。
- `src/web/assets/render/explorer.js`：选择前 5、补齐 bucket、生成 SVG、明细元数据与表格。
- `src/web/assets/components.css`：Explorer 结果、small multiple 和 details/table 组件样式。
- `src/web/assets/charts.css`：SVG 线、点、网格和坐标标签样式。
- `src/web/assets/copy.js`：新增中英文用户可见文案。
- `src/web/mod.rs`：静态资产和 DOM/渲染契约测试。

## 10. Compatibility And Rollback

这是纯前端增量变更，没有数据迁移。live 与 snapshot/export 复用同一 shell 和 renderer，
因此不增加模式分支。若视觉实现无法通过响应式或可访问性检查，可整体恢复原
`#explorer-series` 表格 host；后端 payload 和用户数据库不受影响。

## 11. Rejected Alternatives

### 只给表格加内部滚动

能压缩高度，但用户仍需逐行阅读才能理解趋势，不满足已确认的趋势图目标。

### 五条折线重叠

同一坐标系便于绝对比较，但弱势维度容易被压平，窄屏和单强调色体系下辨识成本高。

### 堆叠柱/面积图

部分 session、tool 等分组可能重叠，不保证跨维度可加；堆叠会暗示一个不稳定的总量语义。

### 为每个维度分配不同颜色

违反现有单强调色/语义色规则，也让颜色承担唯一识别职责。

### 分页或虚拟列表

超出本次“默认页面高度与趋势理解”范围；当前 80 行可见上限配合显式提示足以控制 DOM。
