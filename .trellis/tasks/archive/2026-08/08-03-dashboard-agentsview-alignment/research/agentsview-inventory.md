# AgentsView 参考实现清单（ref/repo/agentsview）

来源：Explore 代理对 `ref/repo/agentsview` 的深度扫描（2026-08-03）。

## 1. 技术栈

| 层 | 选型 |
|---|---|
| 后端 | Go 1.26，Huma v2 over `net/http.ServeMux`，OpenAPI 自动生成 |
| 前端 | Svelte 5（runes）、Vite、TypeScript |
| 组件库 | `@kenn-io/kit-ui`（含基础设计 token） |
| 图表 | **无图表库**，全部手写内联 SVG |
| 存储 | SQLite 为主（`~/.agentsview/sessions.db`），可选 PostgreSQL/DuckDB 镜像 |

## 2. 页面结构

路由手写（History API）：`/sessions`（主三栏视图；未选会话时中栏即 Analytics 看板）、`/usage`、`/activity`、`/trends`、`/recall`、`/insights`、`/pinned`、`/trash`、`/recent-edits`、`/data`、`/settings`。

- Analytics 看板**没有独立路由**，是 `/sessions` 的空态。
- 搜索不是页面，在 Ctrl+K 命令面板中（<3 字符本地过滤，≥3 字符走服务端 fulltext/semantic/hybrid）。
- 日期范围跨页共享（yokedDates store）：Analytics ↔ Usage ↔ Activity ↔ Trends ↔ Insights 共用一个范围。

### Analytics 看板组合（AnalyticsPage.svelte）

sticky 工具栏 + 活动过滤 chips，然后 `1fr 1fr` 网格（`gap: 12px`，`.wide` 占满整行），顺序：
Heatmap(wide) → Activity by Day and Hour 卡（ActivityTimeline + 分隔线 + HourOfWeekHeatmap）→ TopSessions → ProjectBreakdown(wide) → SessionShape → ToolUsage → TopSkills(wide) → SkillTrend(wide) → VelocityMetrics(wide) → AgentComparison(wide)。≤760px 塌成单列。11 个并行 API 请求。

## 3. 核心部件规格

### 统计卡 SummaryCards

六张：Sessions / Messages / Projects / Active Days / Messages-per-Session（sublabel `med {median} / p90 {p90}`）/ Concentration（百分比 + 最活跃项目名）。

```css
.summary-cards { display: flex; gap: 8px; flex-wrap: wrap; }
.card { flex: 1; min-width: 120px; padding: 12px; }
.card-value { font-size: 20px; font-weight: 600; color: var(--text-primary); line-height: 1.2; }
.card-label { font-size: 11px; color: var(--text-muted); font-weight: 500; }
.card-sub   { font-size: 10px; color: var(--text-muted); margin-top: 2px; }
.card.featured { border-width: 2px; border-color: var(--accent-blue); }
```

### GitHub 式贡献日历 Heatmap.svelte

- `CELL_SIZE=16`、`CELL_GAP=2`（步长 18）、左侧标签槽 36px、顶栏 16px、`<rect rx="2">`，SVG 总高 146px。
- 列在周日断行；月份标签在月变化且 weekday index ≤3 时输出；星期标签 `["","Mon","","Wed","","Fri",""]`，9px，`fill: var(--text-muted)`。
- 绿色标尺 = GitHub 原版，按 `:root.dark` 切换：
  - light: `["var(--bg-inset)", "#9be9a8", "#40c463", "#30a14e", "#216e39"]`
  - dark: `["var(--bg-inset)", "#0e4429", "#006d32", "#26a641", "#39d353"]`
- level 0 是主题 inset 背景，不是绿色。level 由服务端算好。
- 三态切换 Messages / Sessions / Output Tokens。点击某天下钻（把日期范围收缩到那天）；选中格 `stroke: var(--text-primary); stroke-width: 2`。

### 7×24 小时热力图 HourOfWeekHeatmap.svelte

- 7 行 × 24 列，`CELL_SIZE=17`、gap 2、`rx=2`，489×155px，行标签槽 29px，列标签高 18px。
- 同一套 GitHub 绿标尺，但 level 在**客户端**按网格最大值的 25/50/75% 阈值计算。
- 行渲染以周日开头（API Monday=0，有显式重映射）。小时标签稀疏（0,3,6,9,12,15,18,21）。
- 时区完全由服务端分桶：请求携带 `Intl.DateTimeFormat().resolvedOptions().timeZone`，标题旁显示缩短的时区名。
- 点击格/星期标签/小时标签设置 dow/hour 下钻；不匹配格 `opacity: 0.2`，激活轴标签 `var(--accent-blue)` weight 600。

### Top Sessions（TopSessions.svelte）

flex 列表非表格：排名（18px mono 右对齐）· 状态点 · 名称+项目副行 · 指标（mono，`var(--accent-blue)`，`min-width:86px` 右对齐）。三种**服务端**排序：By Messages / By Duration（活跃时长+括号内墙钟）/ By Output Tokens（有 token 数据才显示）。

### 项目条形列表 ProjectBreakdown.svelte

Top 15，尾部折叠为 `Other (N)`。纯 div 条形：

```css
.project-name { width: 140px; font-size: 11px; color: var(--text-secondary); }
.bar-track { flex: 1; height: 14px; background: var(--bg-inset); border-radius: 2px; }
.bar-fill  { height: 100%; background: var(--accent-blue); border-radius: 2px; min-width: 2px; }
.bar-value { width: 52px; text-align: right; font-size: 10px; font-family: var(--font-mono); }
.bar-row.selected { background: color-mix(in srgb, var(--accent-blue) 12%, transparent); }
.bar-row.dimmed { opacity: 0.35; }
```

宽度为 max 的内联百分比。统一平蓝色（多彩色只出现在 Usage 页 treemap/时序）。

### 其他图表

- ActivityTimeline：SVG 柱图，`BAR_HEIGHT=120`、`MIN_BAR_WIDTH=6`、`BAR_GAP=2`，ResizeObserver 响应式，25/50/75/100% 四条虚线参考线，`.bar { fill: var(--accent-blue); opacity: .8 }`。
- CostTimeSeriesChart：堆叠面积图，`CHART_H=180`、`MAX_SERIES=5` 尾部折叠为灰色，`niceScale()` 取 1/2/5×10ⁿ 刻度，面积 `opacity=.7`，8px 圆形图例点。
- Treemap：squarified 布局，tile `rx=3`，按 tile 大小渐进披露标签。
- TrendsLineChart：`HEIGHT=300`，每序列画两遍（16px 隐形命中区 + 可见线），激活线宽 3，非激活 `stroke-opacity: .24`。

### 控件

- RangePicker：预设 7d/30d/90d/1y/All + 日/周/月 + 自定义；"最近 N 天"含今天。
- 模型过滤：多选下拉。
- 命令面板：560px 宽、max-height 400px、`padding-top: 20vh`。
- Export CSV：仅 Analytics 页（summary/activity/projects/tools/velocity）。
- 主题切换：header 图标；`.dark` 加在 `:root`；另有高对比度模式、图表色板选择、文字缩放。
- 约定：**SSE 事件从不触发自动刷新**，只置 `hasNewData` 标志，由用户显式刷新。

### 图表 tooltip（各图表复制同一段）

```css
.tooltip { position: fixed; transform: translateX(-50%) translateY(-100%);
  padding: 4px 8px; background: var(--text-primary); color: var(--bg-primary);
  font-size: 10px; border-radius: var(--radius-sm); z-index: var(--z-tooltip); }
```

## 4. 设计 token（kit-ui brand.css + app.css）

### Light（`:root`）

```css
--bg-primary:#f5f6f8; --bg-surface:#ffffff; --bg-surface-hover:#f0f1f4; --bg-inset:#ecedf2;
--border-default:#d8dae2; --border-muted:#e4e6ec;
--text-primary:#181b24; --text-secondary:#555b6e; --text-muted:#878ea0;
--accent-blue:#2563eb; --accent-amber:#d97706; --accent-purple:#7c3aed;
--accent-green:#059669; --accent-red:#dc2626; --accent-teal:#0891b2;
```

### Dark（`:root.dark`）

```css
--bg-primary:#0d0d12; --bg-surface:#16161e; --bg-surface-hover:#1f1f2a; --bg-inset:#111116;
--border-default:#333345; --border-muted:#2a2a3a;
--text-primary:#ececf1; --text-secondary:#b4bcd0; --text-muted:#8b93a8;
--accent-blue:#60a5fa; --accent-amber:#fbbf24; --accent-purple:#a78bfa;
--accent-green:#4ade80; --accent-red:#f87171; --accent-teal:#22d3ee;
```

### 排版与几何

```css
--font-sans: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
--font-mono: "JetBrains Mono", "SF Mono", Menlo, Consolas, monospace;
/* root 字号 13px（--font-size-md: .8125rem）；2xs .625 / xs .6875 / sm .75 / lg .875 / xl 1.125 / 2xl 1.5rem */
--space-1..8: 2,4,6,8,12,16,24,32px;
--radius-sm:4px; --radius-md:6px; --radius-lg:8px;
--shadow-sm:0 1px 2px rgba(0,0,0,.05); --shadow-md:0 2px 8px rgba(0,0,0,.08); --shadow-lg:0 4px 16px rgba(0,0,0,.1);
--header-height:44px; --status-bar-height:24px;
--focus-ring:2px solid var(--accent-blue); --transition-fast:.12s;
```

body 设 `font-feature-settings: "cv11","ss01"`；滚动条 6px。浮层统一样式：`background: var(--bg-surface); border: 1px solid var(--border-default); border-radius: var(--radius-md); box-shadow: var(--shadow-lg)`。

### 分类图表色（六色，做过色盲区分验证）

```
light: #2a78d6 #1baf7a #eda100 #008300 #4a3aa7 #e34948  other: #898781
dark:  #3987e5 #199e70 #c98500 #008300 #9085e9 #e66767  other: #898781
```

### 代理身份色（utils/agents.ts，45 个映射，节选）

claude→blue，codex→green，copilot→amber，opencode/kilo→purple，cursor→black，gemini→rose，kimi→pink，pi→indigo，antigravity→violet，grok（未列出，可归 other）。

源徽章刻意低调（**非填充胶囊**）：

```css
.agent-tag { font-size: 8px; font-weight: 600; text-transform: uppercase;
  letter-spacing: .02em; line-height: 1; opacity: .7; max-width: 52px;
  overflow: hidden; text-overflow: ellipsis; }
/* style:color = 代理对应 accent 色 */
```

### 布局

- 三栏：侧栏默认 260px（可拖 220–520px，localStorage 持久化），右 vitals 栏固定 320px；内容高 `calc(100vh - 44px - 24px)`；≤760px 侧栏变 280px overlay 抽屉。
- 侧栏会话行：42px 高（紧凑 34px），激活态 `color-mix(in srgb, var(--accent-blue) 11%, ...)` + 3px 左侧竖条 + inset 1px 描边；名称 12px/450，meta 10px muted。手写虚拟化（overscan 10，rAF 节流）。
- 顶栏：左 logo+wordmark（12px/650）+ 项目 typeahead；中间搜索框（26px 高、`--bg-inset`、`⌘K` 徽章）；右侧动作簇（Sync/Import/主题/设置），`.header-btn` 28×28、`--radius-sm`。
- Tab 顺序：Sessions · Usage · Activity · Trends · Recall · Pinned · Insights · Trash · Recent Edits · Data。
- `.pill.active { background: color-mix(in srgb, var(--accent-blue) 12%, transparent); color: var(--accent-blue); font-weight: 600; }`

## 5. 数据与同步（对照参考）

- 直接解析磁盘文件：Claude `~/.claude/projects/**/*.jsonl`、Codex `~/.codex/sessions`、OpenCode storage 等 60+ CLI，注册表在 `internal/parser/types.go:124`。
- SQLite 单库；金额一律 int64 微美元；成本 = input+output+cacheWrite+cacheRead 各自费率求和，最后统一舍入一次；未定价模型显式暴露而非静默按零。
- 增量同步：mtime/指纹跳过 → 追加字节游标 → parse-diff 最小写；fsnotify 去抖。
- 看板相关 API：`/api/v1/analytics/{summary,activity,heatmap,projects,hour-of-week,top-sessions,...}`、`/api/v1/usage/*`；所有分析端点共享同一个过滤结构（from/to/timezone/project/agent/model/dow/hour/...），因此每页过滤栏一致。

## 6. 产品意图（PRODUCT.md / DESIGN.md）

明确反参考：不要营销 hero、不要装饰性 SaaS 看板、不要用超大卡片掩盖密度。原则："会话数据优先、维护 local-first 信任、重复工作流保持紧凑可预测"。

## 7. 关键文件路径（后续实现引用）

- token：`frontend/src/app.css`；基础色板在 kit-ui 仓库 `src/lib/brand.css`（本地未 vendor）
- 看板组合：`frontend/src/lib/components/analytics/AnalyticsPage.svelte`
- 部件：`analytics/{SummaryCards,Heatmap,HourOfWeekHeatmap,TopSessions,ProjectBreakdown,ActivityTimeline}.svelte`
- Usage：`usage/{UsagePage,CostTimeSeriesChart,Treemap}.svelte`
- 布局：`layout/{AppHeader,ThreeColumnLayout}.svelte`
- 身份色：`lib/utils/agents.ts`；CSV：`lib/utils/csv-export.ts`
