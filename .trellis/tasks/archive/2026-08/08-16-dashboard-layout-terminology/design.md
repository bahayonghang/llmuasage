# Design：用量概览布局与术语优化

## Boundaries

本任务限制在嵌入式 Web 看板展示层：`src/web/shell.rs`、`src/web/assets/*.css`、`src/web/assets/copy.js`、相关展示渲染器、必要的展示测试与 Dashboard 文档。渲染器 ID、既有 i18n key、API 字段和快照数据结构保持兼容。

## Layout Design

保留 `.panel.ready-widget-panel.wide.analytics-heatmaps` 的单面板结构，在宽屏时把三个直接子元素排成：

```text
每日活跃度  |  每周活跃时段
minmax(0,1fr)  minmax(489px,1fr)
```

- 分隔线在宽屏为 `1px` 纵线，随网格行高伸展。
- `.hour-week-svg` 使用 `width: 100%`、`height: auto` 和 `min-width: 489px`：空间充足时放大利用分区宽度，空间不足时由既有 `.heatmap-scroll` 产生局部横向滚动。
- 在中等视口断点恢复单列，分隔线恢复为横线；移动端沿用既有标题/控件纵向布局。
- 日历 SVG 继续按日期范围计算固有宽度。40 周以上的长范围在宽屏分区内等比缩放并保留 `640px` 最低可读宽度，消除全年范围的多余横向滚动；单日或短范围仍保持固有尺寸，避免格子被夸张拉伸。

## Terminology Model

展示层采用以下统一概念，不改内部字段名：

| Internal / current wording | User-facing concept |
| --- | --- |
| `source`, `platform`, “平台” | 来源 / Source |
| `cache_efficiency` | 缓存读取占比 / Cache-read share |
| `cursor_count` | 同步游标 / Sync cursors |
| `failure_count` | 最近失败 / Recent failures |
| Activity calendar | 每日活跃度 / Daily activity |
| Day and hour | 每周活跃时段 / Weekly activity |
| Top sessions | 高用量会话 / Highest-usage sessions |
| event count in usage widgets | 请求 / Requests |

中文 locale 对内部实现词进行用户化翻译；必须保留的技术专名包括 Token、CSV、SQLite、USD、MCP、HTTP、URL、ID、产品名和命令名。英文 locale 只同步修正语义不准确或含糊的名称。

## Copy Synchronization

- `copy.js` 是运行时中文/英文文案权威来源。
- `shell.rs` 中的中文默认值与 `SHELL_COPY_ZH` 同步，避免 JS 启动前闪现旧术语。
- 行为分析、用量分析和诊断线索的动态标签使用 `UI_COPY`，并按稳定 ID 或状态值映射；后端英文原因和 API 枚举保持不变。
- CSV 表头与看板摘要名称同步。
- 中英文 Dashboard 文档使用相同部件名称。

## Compatibility and Rollback

- 不新增资产，不改 `ASSET_MANIFEST`。
- 不改 DOM ID、i18n key 或渲染函数签名，旧快照与 live 页面继续走现有模块图。
- 布局可通过回退相关 CSS 和文案改动完整撤销；没有数据迁移或持久化影响。

## Validation Strategy

- Rust：扩展 `overview_wide_layout_avoids_orphan_blank_columns` 或相邻测试，固定双栏/回退/响应式 SVG 和关键术语契约。
- Node：运行现有 Dashboard 生命周期、CSV、加载与 fetch 测试；必要时增加文案映射断言。
- 浏览器：用隔离 fixture 启动本地看板，在宽屏与移动视口检查几何、页面横向溢出、中英文和深浅色。
- 文档：构建中英文 VitePress 页面。
