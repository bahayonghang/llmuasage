# 优化 serve 网页看板排版、样式与性能（父任务）

## Goal

修复 `llmusage serve` 网页看板经代码审计确认的排版、样式、前端渲染与后端数据路径问题。本目录为**父任务**，不直接启动实施；实施拆分为 5 个可独立验收的子任务（见下），另有一批项移入 backlog。

## Background

立项依据：真实运行截图 + 对 `src/web/`、`src/web/assets/`、`src/query/`、`src/store/` 的三路代码审计（47 项已确认问题，见文末附录）。规划经一轮独立评审后收敛，主要修正：

- R8 原表述自相矛盾（要求新增 `generated_at` 又要求契约字段不增不减），且 `overview.generated_at` 每次查询都是 `now_utc()`（src/query/mod.rs:829/2256），不能当脏检查指纹 → 改为**客户端语义指纹**，明确不改契约。
- R9 原表述未限定缓存层级，可能违反 cold home overview 契约（`home_overview` 直接调 `Dashboard::diagnostics()`，src/query/home_overview.rs:180；spec 禁止 process cache）→ 缓存限定在 **WebState 层**。
- R10 原并发表述不准确：full 路径是先 await core（1 permit），再 `tokio::join!` 5 个 secondary（src/web/mod.rs:952-997），并发需求 5、信号量 4，第 5 个排队；且 behavior 四 section 有 1s 超时 + 逐 section 降级（mod.rs:41,902），`Dashboard::snapshot()` 是导出用顺序执行、任一错误整体失败（src/query/mod.rs:2332），不能直接替换 → 改为**先测量，再设计保留降级能力的 live 专用组合器**；N+1 拆为独立条目。
- 性能验收原指向 docs fixture，但其默认只 seed 12 行（examples/docs_dashboard_serve.rs:19），实测 interactive p95 仅 5.68–8.17ms，无法覆盖 stat 风暴/N+1/并发 → 采用**三数据集验证策略**。
- 原"语言切换不全量重渲"目标错误（locale 天然影响全部文案）→ 目标改为**复用派生 context，只更新文案容器**；原 R7"收集齐统一渲染"违反 secondary 逐 section 独立语义 → 改为**renderBehavior 拆 section 级渲染**。
- A1 原断言"无横向滚动条"不足以抓破版（1366×768 实测无文档级滚动条，但筛选栏内部溢出 ~5px、按钮逐字竖排）→ 断言加强。

约束本任务树的既有契约：

- `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`：interactive 预算（点击反馈 p95 ≤100ms、interactive API p95 ≤400ms、JSON ≤128KiB）、secondary 并发 2 且只更新自己的 section、live 缓存 10s/32 条、4 个查询 permit + InterruptHandle、full/core scope 形状保持兼容、cold home overview 80ms 预算禁止 process cache。
- `.trellis/spec/llmusage/backend/web-server-contracts.md`：serve 监听/浏览器策略不动。
- 设计系统：local-first instrument，catppuccin token，禁止通用 SaaS 看板化处理。

## 已定产品决策（实施前可推翻）

- **D1 KPI sparkline**：删除硬编码假曲线（hero.js:110-112），连带清理 `buildSparkline` 死代码（derive.js:563-579）。真实序列 sparkline 如需另立增强任务。
- **D2 导航 badge**：删除静态硬编码 "4"/"24h"（shell.rs:78,83），不做动态计数。
- **D3 主题默认值**：首访跟随 `prefers-color-scheme`，无偏好时保持 light；防闪烁内联脚本与 theme.js 收敛为单一事实源。
- **D4 移动端健康卡**：不恢复 720px 以下的大系统健康卡，改为在首屏提供紧凑/折叠的健康摘要入口（不再是 `display:none` 后彻底失联）。

## 子任务地图

| 子任务 | 范围 | 优先级 | 依赖 |
| --- | --- | --- | --- |
| `07-22-serve-dash-responsive-i18n` (S1) | 破版与告警/i18n 正确性：中间断点、warn 视觉、数字口径、KPI i18n、移动端健康摘要 (D4) | P0 | 无 |
| `07-22-serve-dash-render-lifecycle` (S2) | 渲染生命周期：dirty-check、调用路由、buildContext/formatter 复用、renderBehavior 拆分、自动刷新 interactive 化、job 轮询 | P1 | 无（建议 S1 后） |
| `07-22-serve-dash-query-path` (S3) | 后端查询路径：先测量；WebState stat 缓存、full scope 门槛决策、compare N+1、busy_timeout | P1 | 无，可与 S2 并行 |
| `07-22-serve-dash-http-cache` (S4) | HTTP 传输：ETag/Cache-Control、压缩、根 HTML 缓存 | P1 | 无，独立快赢 |
| `07-22-serve-dash-visual-polish` (S5) | 视觉打磨：hero 留白、CJK 字距、D1/D2/D3、SVG 变形、对比度 | P2 | 建议 S1 后（同文件多） |

启动顺序建议：S1 → S4（快赢）→ S2 ∥ S3 → S5。每个子任务在自己的目录内有独立 PRD 与验收；S2/S3 另有 design.md。

## Backlog（本任务树不做，另立项）

- B1（原 R18）CSS 死代码清理与 token 收敛：重复 `.panel-title`、无引用 `.grid-2`/`cost-grid`、断点重复段、硬编码颜色（shell.rs:276-278、layout.css:187）、palette 直用、行尾统一。
- B2（原 R19）copy.js 瘦身：死导出删除、locale 动态拆包、logger.info debug 开关。
- B3（原 R20 部分）"展开完整排行"行数上限、escapeHtml 去重（app.js:419 vs format.js:1）。
- B4（原 R21）JS/CSS minify 或合并——须先 ADR 讨论是否引入构建步骤。
- B5 breakdown 查询 LIMIT 与 payload 分段（src/query/mod.rs:933-968）。
- 理由：locale 动态拆包、minify、整文件行尾转换目前无性能证据支撑，且易造成大范围 diff；在 S1–S5 落地后按实测需要再启动。

## 全局硬性约束（所有子任务继承）

- H1：full/core scope 响应形状向后兼容；interactive 契约字段不增不减（脏检查用客户端语义指纹实现，见 S2）；遵守 dashboard-performance-contracts 全部条款；cold home overview 路径不得引入 process cache（见 S3）。
- H2：不改 serve 监听/浏览器启动策略；不改 sync 写路径；schema migration 不在本任务树范围。
- H3：保持设计系统，不引入第三方前端依赖/CDN。
- H4：三数据集验证——docs fixture 负责视觉/契约；代表性真实数据库只读副本（`scripts/benchmark-dashboard-range.mjs`）负责性能预算；专用 stress fixture 负责缓存失效与并发。
- H5：UI 变更同步更新 docs/dashboard 中英文页面与截图；`just ci` 全绿是每个子任务的完成门槛。

## 父任务验收（全部子任务完成后核对）

- [x] 5 个子任务均 archive 且各自验收项通过。
- [x] 汇总证据：viewport 矩阵截图、benchmark 报告、304/压缩 curl 证据、stat 计数证明、`just ci` 记录。
- [x] docs/dashboard 中英文页面反映最终状态；spec 有新增约定时回写 `.trellis/spec/llmusage/backend/`。

## 附录：审计 Confirmed Facts（47 项，标注归属）

### A. 布局与排版

1. [S1][高] 筛选栏 1101–1450px 视口横向溢出：`layout.css:120-130` 六列网格最小宽 ≈1122px（含 `.filter-range-group` min-width 270px，layout.css:172-174），断点只有 1100px/720px。1366×768 实测：无文档级滚动条，但容器内部溢出 ~5px，"应用筛选/重置"被压成逐字竖排。
2. [S1][中高] 告警横幅零告警视觉：JS 写 `data-tone="warn"`（render/sync-command-center.js:346），CSS 无容器级样式（components.css:301-307）。
3. [S5][中] Hero 区中部 ~350px 死白：layout.css:59-66（`1fr 360px`）+ hero-desc `max-width:520px`（layout.css:80-85）。
4. [S5][中] CJK 被 letter-spacing 误伤：components.css:738-748、layout.css:333-341、layout.css:137-144；截图可见"近 24 小 时""同 步"被拉开。
5. [S1][中] explorer-controls 720–1100px 溢出：components.css:1253-1263 固定 4 列，中间断点缺失。
6. [S1][中低] topbar 720–1100px 不换行：layout.css:26-57，仅 720px 有 wrap（layout.css:630-634）。
7. [S1][中] 数字格式化口径不统一：sync-command-center.js:89-101 直出原始数字，hero.js:86 走 `formatNumber`。
8. [S1][中] KPI 文案硬编码在数据层：data/derive.js:447-481 写死中文，英文 locale 失效；shell.rs:35 `lang="zh-CN"` 静态。
9. [S5][中] KPI sparkline 硬编码假曲线：hero.js:110-112；derive.js:563-579 `buildSparkline` 死代码。→ D1。
10. [S5][中] 趋势图 SVG `preserveAspectRatio="none"` 拉伸变形：shell.rs:269 + charts.css:1-8；explorer 迷你图已用 `vector-effect` 而它没有。
11. [S5][低] 导航激活 pill 全反色成全页最亮元素：components.css:68-71。
12. [S5][低中] 同步中心 metric 单元格宽屏过度拉伸（components.css:399-409）；详情 summary 提示语居中漂浮（components.css:438-449）。
13. [S5][低] 侧边栏下部大片空白：components.css:102-106 footer `margin-top:auto`。
14. [S5][低中] 导航 badge "4"/"24h" 静态硬编码：shell.rs:78,83。→ D2。
15. [S1][低] 移动端 ≤720px `display:none` 隐藏系统健康卡：layout.css:613-615，健康信息彻底失联。→ D4。
16. [S1][低] "应用筛选"无 btn-primary 强调、筛选列宽比失衡：shell.rs:227-230、layout.css:122。

### B. 样式架构

17. [B1][中] 死代码/重复规则成片：`.panel-title` 双定义（components.css:1032 vs 2063）、`.grid-2`/`cost-grid` 无引用（layout.css:361-366,438-443）、720px 断点规则两文件重复（layout.css:539-576 vs components.css:2105-2163）、`.kpi` 无效 transition（components.css:726）。
18. [B1][中低] 硬编码颜色/旧 palette 残留：shell.rs:276-278 `fill="#c8553d"/"#8d867a"`；layout.css:187 硬编码白色 inset 高光；shell.rs:164-178 SVG 内联 style。
19. [S5][中低] 主题不支持系统偏好、默认逻辑两处重复：theme.js:4、shell.rs:35-53 vs theme.js:10-17。→ D3。
20. [S5][低] 主题切换过渡不一致：仅 html/body 有背景过渡（base.css:146），surface 组件斑块式变色。
21. [S5][中低] 无字号/间距体系，17 级字号含 5 个半像素值，padding 各说各话。（S5 内仅做触及处的局部收敛，不做全站 token 化）
22. [S5][低] 中文标签套 mono 字体栈（layout.css:143/340、components.css:888）；KPI 大值 mono 但脚注数字 sans（components.css:754-780）。
23. [S5][中低] 可访问性：暗色 `--muted-2` #7f849c 对比度 ~3.4:1/~2.9:1；数据标签普遍 9.5–11.5px。
24. [B1][低] palette token 直接当语义色用：charts.css:31/36/40。
25. [B1][低] 琐碎无效声明：base.css:142 Inter 私有特性；charts.css:91-94 无效规则；components.css 混 LF/CRLF。

### C. 前端性能

26. [S2][高] 所有面板 innerHTML 全量替换，零 dirty-check。
27. [S2][高] renderDashboard 被过度调用：面板展开/折叠（app.js:1192-1194）、explorer 应用（app.js:1130）、语言切换（app.js:113-117）都全量重渲。注意：locale 切换的目标不是"不重渲"，而是复用派生 context、只更新文案容器。
28. [S2][中] buildContext 一次动作最多重复 ~10 次：app.js:196-216、906-937、245-249。
29. [S2][中] Intl.NumberFormat 每次新建：data/format.js:11/31/43/63。
30. [S2][中] fast-range 6 请求 + secondary 每到一个调一次 renderBehavior 整体重渲 4 个 section（app.js:910-940）。正确方向：renderBehavior 拆 section 级渲染（behavior.js:241-269 已按子容器写 DOM，但一个函数管全部 4 section），不是"收集齐统一渲染"（违反 secondary 逐 section 契约）。
31. [S2][中] 自动刷新=全量重取 full scope + 全量重渲：app.js:962-973 → 852-884。
32. [S2][中] job 轮询无上限无退避，每 900ms 重建整个同步中心面板：app.js:1433-1443。
33. [B2][低] 生产路径 47 处 logger.info。
34. [B3][中] "展开完整排行"无行数上限：models.js:19、projects.js:19、costs.js:22。
35. [B2][中] copy.js 41KB ~85% 双语静态字符串表全量常驻；3 个死导出。
36. [S2][低] sync-command-center 每次渲染逐节点绑 click：sync-command-center.js:372-376。
37. [S5][低] 无限 pulse 动画作用在 box-shadow 上常驻：components.css:127。
38. [B3][低] escapeHtml 重复定义等零星死代码。

### D. 后端 / serve 数据路径

39. [S3][高] diagnostics 对 source_file 全表逐行 `Path::exists()` stat，每次 dashboard 加载/范围切换/自动刷新都执行、无缓存：src/query/mod.rs:2736-2759，被 core_snapshot(2363)/interactive_snapshot(2387) 调用。
40. [S3][高] full scope 连接扇出：先 core（1 连接），再 5 个 secondary `tokio::join!`（src/web/mod.rs:952-997），并发需求 5、permit 仅 4（mod.rs:42）→ 第 5 个排队，第二标签页并发请求可能级联超时。注意：`Dashboard::snapshot()`（query/mod.rs:2332）是导出用顺序单连接、无逐 section 降级，不能直接替换；behavior 四 section 有 1s 超时 + 降级（mod.rs:41,902）。
41. [S4][中] 静态资源零缓存头零压缩：assets/mod.rs:13-16；src/web/mod.rs:92-118 无 compression layer；Cargo.toml:56 tower-http 仅 fs。
42. [S3][中/低] 每 API 请求新开 SQLite 连接含 4 条 PRAGMA（store/connection.rs:21-36）；web 读连接 busy_timeout 30s 与 5s API 超时语义不匹配。
43. [S3][中] compare_model_candidates N+1：src/query/mod.rs:2479-2491，最多 25 个 model 各一条查询。
44. [B5][低] breakdown 查询无 LIMIT，full scope payload 随数据量线性增长。
45. [S2][中] sync 完成后立即 full scope 重载：app.js:1473-1475，WAL 未 checkpoint 时最易触发 5s 超时。
46. [S4][低] 根页面 HTML 每请求重新 format! ~15KB：src/web/mod.rs:168-170。
47. [B4][低中] 未 minify：JS ~180KB + CSS ~66KB；16 个 JS 模块 + 4 个 render-blocking CSS，module 瀑布深 3 层无 preload。
