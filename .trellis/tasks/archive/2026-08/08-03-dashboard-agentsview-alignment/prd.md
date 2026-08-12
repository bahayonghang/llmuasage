# PRD：看板对齐 AgentsView —— 功能补齐与视觉重构（父任务）

## 背景

用户要求以 `ref/repo/agentsview`（Go + Svelte 的多代理会话看板）为参考：

1. 分析 llmusage 网页看板相对 AgentsView 缺失的本体功能并补齐；
2. 将网页样式与排版对齐 AgentsView 的视觉体系。

深度差距分析见 `research/gap-analysis.md`；两侧完整清单见 `research/agentsview-inventory.md` 与 `research/llmusage-web-current-state.md`；**外部审阅核验出的真实契约事实见 `research/review-verification.md`（各子任务 design 的权威依据）**。

核心事实：`/api/home_overview`、`/api/heatmap`、`/api/trends_daily`、`/api/compare/models` 已实现已路由但前端零调用；Top Sessions 需新建 SQL 聚合查询（现有 `load_session_report` 缺 model/project_hash 过滤且绕过 Dashboard 门面，不可复用——见 review-verification §1）；7×24 小时热力图需新查询；快照导出需扩展 `DashboardSnapshot` DTO。

## 需求范围（父任务 owns，交付拆到子任务）

### R1 视觉系统对齐（→ 08-03-dashboard-visual-system）

设计 token、排版、卡片/导航/徽章/tooltip 对齐 AgentsView。边界：CSS + shell.rs + 渲染器最小 markup 改动（源徽章需 `data-source` 属性，纯 CSS 无法按源着色——review-verification §9）。本任务显式定义供后续任务引用的 token 名（源身份色、分类图表色、热力标尺）。

### R2 接入已就绪端点（→ 08-03-dashboard-ready-widgets）

六卡汇总统计行（`/api/home_overview` 的 `summary`+`by_platform`，真实字段见 review-verification §6）、GitHub 式贡献日历（`/api/heatmap`）、每日多序列趋势（`/api/trends_daily`）。**范围修正**：无新 HTTP 端点，但含三类必要后端/基建改动 —— ① `DashboardSnapshot` DTO 扩展三个可选字段 + 组装 + 旧快照兼容（§3）；② 新面板并入 `SECONDARY_SECTIONS` latest-wins 加载生命周期 + node 测试扩展（§4）；③ schema v20 增加单个 event-at-first covering expression index，使 compact home overview 在代表性大库满足 400ms 交互预算。该例外不改表数据或写路径，也不增加第二索引。

### R3 时区基础（→ 08-03-dashboard-timezone-iana，轻量）

`ReportTimezone::Iana(Tz)` + HTTP 解析接受 IANA 名 + 前端携带浏览器时区参数。决策记录：采纳审阅建议支持任意 IANA 时区，理由是 chrono-tz 已是依赖、`ResolvedZone::Iana` DST 机制完备、零新依赖（§5）。

### R4 会话分析（→ 08-03-dashboard-session-analytics）

- 新建 `Dashboard::top_sessions` SQL 聚合查询（完整 QueryFilter、SQL 侧排序+LIMIT、session_id 稳定 tiebreak）+ `/api/sessions` 路由 + Top Sessions UI。
- `LogsQuery` 扩展：服务端 `session` 过滤 + `event_key` 单记录 raw 详情契约（§2）+ 日志查看器 UI。
- 7×24 小时热力图：Rust 侧 dow/hour 折叠（按 `hour_start` 聚合后逐行 `ResolvedZone` 转换）+ `/api/hour_of_week` 路由 + UI。依赖 R3。
- Analytics CSV 导出：含公式注入防护（`^[=+\-@\t\r\n]` 前缀单引号，对齐 AgentsView 实现），纯函数测试必选（§8）。

## 非目标（明确排除）

- 转录查看器、内容搜索、Recall、Insights、Pinned、Trash、Recent Edits、Data 页 —— llmusage 不存消息正文，无数据基础。
- 不引入前端框架、构建步骤、webfont 网络下载、客户端路由；维持 vanilla ES modules + 内嵌资产架构。
- 不移除 llmusage 独有功能（Sync Command Center、Cost Explorer、Optimize、Compare、静态导出、双语）。
- 不改动同步/解析/存储写路径；后端改动限于只读查询、路由、快照 DTO、时区解析，以及 ready-widgets 已批准的单个 v20 查询覆盖索引。

## 跨子任务验收标准（父任务最终集成审查用）

1. `just ci` 全绿（fmt/clippy/test/doc + node --check/--test + docs build）。
2. **性能预算继承**（review-verification §7）：交互 API p95 ≤ 400ms、交互 JSON ≤ 128 KiB、点击反馈 ≤ 100ms；home_overview 冷读 80ms×3 测试不回归；logs 分页 < 30ms/页。
3. `cargo run -- serve` 后看板在 light/dark × zh/en 四组合下无布局破损；新部件空库时显式空态而非报错。
4. `export html` 快照模式下新部件正常渲染或优雅降级；**旧版 snapshot.json（缺新字段）加载不报错**。
5. `--public` 模式不暴露任何新增 loopback 端点（`/api/sessions`、`/api/hour_of_week` 均 404）。
6. 新面板全部走 latest-wins 生命周期：快速连续切换过滤/范围无旧数据覆盖（手工验证 + node 测试）。
7. 视觉抽查对照 `research/agentsview-inventory.md` §3–4 规格。
8. `docs/dashboard/index.md` 与 `docs/zh/dashboard/index.md` 更新；新端点补进集成端点清单（`docs/prd/llmusage-integration-prd-v1.1.md` 若为权威清单）。

## 最终集成审查与归档顺序（可执行清单）

1. 全部四个子任务各自 `trellis-check` 通过并按其 implement.md 提交后，回到父任务视角跑一次全量 `just ci` + 上表 1–8 逐条走查。
2. 重拍 `docs/dashboard` 截图（夹具：`cargo run --features testing --example docs_dashboard_serve -- --port 37421`，1440×1100）。
3. Phase 3.3：`trellis-update-spec` 评估是否更新 `dashboard-performance-contracts.md`（新端点预算）、`web-server-contracts.md`（新路由暴露面）、新增前端资产清单契约。
4. 归档顺序：先四个子任务 `task.py archive`，最后父任务 archive；父任务 archive 前确认 `[4/4 done]`。

## 子任务映射与顺序

| 顺序 | 任务 | 理由 |
|---|---|---|
| 1 | 08-03-dashboard-visual-system | token 先行，定义后续任务引用的 token 名 |
| 2 | 08-03-dashboard-ready-widgets | 依赖 1 的 token/网格；建立新面板加载生命周期模式 |
| 2'（可并行） | 08-03-dashboard-timezone-iana | 独立小任务，无前置；须在 4 之前 |
| 3 | 08-03-dashboard-session-analytics | 依赖 1（token）、2（生命周期模式）、2'（时区契约） |

顺序依赖已写入各子任务 prd.md；父任务本身无直接实现工作，不作为 `task.py start` 对象。
