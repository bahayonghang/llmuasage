# 差距分析：llmusage 看板 vs AgentsView

结论优先：llmusage 已具备 AgentsView 核心看板部件所需的**绝大部分后端能力**，差距集中在前端未消费与视觉体系不一致。功能补齐分三档：

## A 档 — 后端已就绪，仅缺前端（零后端成本）

| AgentsView 部件                                                                     | llmusage 对应能力                                                                 | 状态               |
| ----------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------ |
| 六卡汇总行（Sessions/Messages/Projects/Active Days/Msgs-per-Session/Concentration） | `/api/home_overview`：sessions/requests/tokens/cost/cache 效率/active days/平台数 | 端点在，前端零调用 |
| GitHub 式贡献日历（按月×星期，绿标尺）                                              | `/api/heatmap`：366 天零填充网格，DST 正确                                        | 端点在，前端零调用 |
| 每日多序列趋势（token 类型拆分堆叠）                                                | `/api/trends_daily`：input/cache_read/cache_creation/output + 日成本              | 端点在，前端零调用 |
| 模型选择器候选                                                                      | `/api/compare/models`                                                             | 端点在，前端零调用 |

## B 档 — query 层已算好，缺 HTTP 路由 + UI

| AgentsView 部件                                              | llmusage 对应能力                                                                    | 缺口                                                       |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| Top Sessions（By Messages / By Duration / By Output Tokens） | `src/query/reports.rs` `load_session_report`：每会话 span/活跃分钟/模型拆分/项目归属 | 新增 `/api/sessions` 路由 + 渲染器                         |
| 会话/事件浏览（侧栏会话列表的近似替代）                      | `/api/logs` 游标分页含 session_id/session_label                                      | 已有路由，缺 UI；会话维度聚合可复用 reports                |
| 5 小时计费块 / 上下文压力                                    | `blocks_report` / `context_pressure`                                                 | 新路由 + UI（可选，AgentsView 无对应物，属 llmusage 特色） |

## C 档 — 需要新查询

| AgentsView 部件                                 | 实现途径                                                                                                                                                  |
| ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 7×24 小时活跃热力图（Activity by Day and Hour） | `usage_bucket_30m` 有 `hour_start`，新增 dow×hour 聚合查询 + `/api/hour_of_week` 路由；时区沿用 `QueryFilter.timezone` 服务端分桶（与 AgentsView 同策略） |
| CSV 导出                                        | 前端纯 JS 从已加载快照生成（AgentsView 也是 `csv-export.ts` 前端生成）                                                                                    |

## 不可移植（非目标，写进 PRD）

llmusage 不存储消息正文，以下 AgentsView 功能没有数据基础，明确排除：
转录查看器、全文/语义搜索（Ctrl+K 内容搜索）、Recall（记忆抽取）、Insights（LLM 生成洞察）、Pinned（钉选消息）、Trash（会话软删）、Recent Edits（文件编辑追踪）、Data（项目重分类规则）。

llmusage 独有且保留的功能（AgentsView 没有）：Sync Command Center、Cost Explorer 切片工作台、Optimize 浪费检测、Model Compare、降级 chip 体系、zh/en 双语、静态 HTML 导出。

## 视觉差距

| 维度        | llmusage 现状                          | AgentsView 目标                                                                                                    |
| ----------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| 色板        | Catppuccin（mauve 主色、peach 数据色） | 中性灰蓝 + `--accent-blue`（light `#2563eb` / dark `#60a5fa`）单主色；深色底 `#0d0d12/#16161e`                     |
| 热力/数据色 | 无热力图                               | GitHub 原版绿标尺；条形统一平蓝                                                                                    |
| 字体        | 系统栈                                 | Inter 优先栈 + JetBrains Mono 优先栈（保持系统回退，不引入 webfont 下载）；root 13px                               |
| 卡片        | `.panel/.kpi` 1px 边 + 软阴影          | 更紧凑：统计卡 `padding:12px; min-width:120px`，值 20px/600，标签 11px muted；featured 卡 2px 蓝边                 |
| 半径/阴影   | 项目自有                               | `--radius-sm/md/lg: 4/6/8px`，三档阴影                                                                             |
| 源标识      | 文本标签                               | 8px 大写有色文字徽章（opacity .7，非填充胶囊）+ 每源身份色（claude 蓝/codex 绿/opencode 紫/kimi 粉/pi 靛/grok 灰） |
| 布局        | 单长页锚点滚动 + 248px 侧栏            | 保持单页信息架构（不引入路由），但版面网格对齐：看板区 `1fr 1fr` gap 12px、`.wide` 通栏、≤760px 单列               |
| tooltip     | `<title>` 原生                         | 反色 chip（`background: var(--text-primary); color: var(--bg-primary)`，10px）                                     |

## 风险与约束

1. **26 项资产清单**：每个新 JS/CSS 文件必须登记 `src/web/assets/mod.rs` 且更新数组长度字面量。
2. **CI 测试 import 真实模块**：`data/fetch.js`、`load-state.js`、渲染生命周期的导出形状不可破坏；新增 API 调用应走既有 fetch 层扩展。
3. **JS/CSS 编辑走 Bash**（prettier hook 会破坏单引号风格）。
4. **静态导出**：`export html` 复用同一资产与 shell，新增部件必须在 `data-mode="snapshot"` 下降级良好（无 fetch 的快照模式）。
5. **公开模式**：`--public` 只暴露 `/api/dashboard` 投影，新部件在 public 模式下需优雅缺席或并入 dashboard 快照。
6. **i18n**：所有新增字符串进 `copy.js` zh/en 双语。
7. **主题双轨**：GitHub 绿标尺需 light/dark 双份；level 0 用 inset 背景。
8. **性能契约**：新增查询遵守 `dashboard-performance-contracts.md`（信号量/超时/降级分节）。
