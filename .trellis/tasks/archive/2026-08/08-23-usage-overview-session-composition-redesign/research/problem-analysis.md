# 用量概览两个问题的根因分析

核对日期：2026-08-23。范围：用户截图标出的“高用量会话”和“每日 Token 用量构成”。

## 结论

这不是两个孤立的 CSS 问题：

1. 高用量模块把内部身份键当成了用户标签，导致数据有值但没有解释力；
2. 构成模块在默认范围主动隐藏有效数据，并且只绘制四个可见通道，没有处理权威总量与已知通道之间的差额。

因此任务需要同时收敛“显示语义、范围适配、Token 总量契约、日志下钻和可访问图表”，但不需要新增统计端点或改动 token 归一化。

## 1. 高用量会话：数据正确，显示语义错误

### 当前数据流

- `src/query/top_sessions.rs:63-75` 返回 canonical session id、session label、project、source、Token、成本、时长和事件数。
- `src/query/top_sessions.rs:77-81` 的内部 `Candidate` 已保留 `first_at`/`last_at`，但 `TopSessionRow` 没有公开这两个可用于人类识别的时间字段。
- `src/web/assets/render/top-sessions.js:30` 将 `session_label || session_id` 直接放进 `<strong>`，同时把 `session_id` 放进 `data-session-id` 用于点击下钻。
- `src/web/assets/render/top-sessions.js:31-34` 点击后通过 canonical id 精确过滤事件日志；这一内部键有用，问题只在于它被暴露成主要内容。

### 为什么 `session_label` 仍然像 ID

当前 `SessionInfo` 只承诺“可选标签，通常是 transcript/file stem”（`src/domain/models.rs:257-265`），并不承诺自然语言标题：

- Codex：文件 stem（`src/parsers/codex.rs:562-578`）；
- Claude：文件 stem（`src/parsers/claude.rs:439-445,538-541`）；
- Kimi Code：session UUID（`src/parsers/kimi_code.rs:502-520`）；
- OpenCode/Grok/ZCode：当前也把 session id 复制为 label（`src/parsers/opencode.rs:477-480`、`src/parsers/grok.rs:348-359`、`src/parsers/zcode.rs:650-653`）。

所以前端无法仅凭 `session_label` 判断它是否对人有意义。基于 UUID 正则做“聪明识别”也会不断漏掉新的 ID 形状。更稳妥的边界是：正常排行不显示任何 session identity/label；使用项目、来源和本地时间描述工作上下文，canonical id 仅保留为下钻键。

### 推荐信息结构

- 标题：`会话消耗排行`（比“高用量会话”更明确地表达比较目的）；
- 主标签：`project_label`，缺失时为本地化 `{Agent} 会话`；
- 副标签：Agent 展示名 + 本地起止时间/最近活跃时间 + 可选事件数；
- 图形：十条横向比较条，条长相对当前第一名，右侧是当前指标的精确值；
- 交互：Token / 活跃时长 / 估算成本仍是三态服务端排序；整行按钮下钻日志；
- 隐私：不读取 prompt、消息正文或 raw JSON，不把标题生成扩展到解析器。

横向条形排行复用 `DESIGN.md:258-263` 的 Data Bars 视觉语言，符合宽面板中的比较任务，也比当前相同视觉权重的十行列表更容易发现数量级差异。

## 2. 每日 Token 构成：默认首屏被主动置空

### 已确认的直接根因

- `src/web/assets/app.js:55` 的默认范围是 `1d`。
- `src/web/assets/data/fetch.js:345-351` 会正常请求 `/api/trends_daily` 并携带当前筛选。
- `src/web/mod.rs:862-870` 正常把该请求交给 `Dashboard::trends_daily`。
- `src/query/mod.rs:1199-1239` 正常按本地日返回 Token 通道和权威总量。
- 但 `src/web/assets/render/trends-daily.js:47-52` 在 `1d` 或 `since == until` 时，无论 `rows` 是否有数据都直接返回提示空态。

截图中的空白因此是确定性的默认行为，不是同步失败、查询失败或数据库没数据。归档任务 `08-03-dashboard-ready-widgets/prd.md` 的 R3 曾明确选择“range=24h 时新图显示空态说明”；当前反馈证明该选择不满足首屏使用需求。

### 第二个潜在空图问题：权威总量不一定等于四通道之和

`src/web/assets/render/trends-daily.js:61` 只对 input/cache read/cache creation/output 求和并据此定柱高。项目 Token 契约要求：

- 持久化 `total_tokens` 是报告总量的权威来源；
- reasoning 默认是诊断通道，不得普遍再次加到 output/total；
- Pi/Oh My Pi/Grok/ZCode 等来源可能采用权威 provider total；Grok 的 total-only fallback 甚至可能四个子通道全为零。

证据见 `.trellis/spec/llmusage/backend/token-accounting-contracts.md:35-37,60-67,80-102,173-176`。因此构成图必须显式表示 `total_tokens - 已知四通道` 的非负差额，命名为“其他/未细分”；不能通过重新求和或把 reasoning 一律并入 output 来填平。

另外，`src/query/mod.rs:139-152` 的注释声称 DailyTrendPoint 的 output 已包含 reasoning，但实际查询 `src/query/mod.rs:1204-1212` 只读取 `SUM(output_tokens)`，且现行 token-accounting 契约禁止默认相加。实施应修正文档注释与测试，而不是把 SQL 改成普遍加 reasoning。

### 推荐范围适配

- `1d`/单日：把返回行聚合成一个 100% 构成条，旁边列精确 Token 与占比；该面板回答“构成”，上方短时趋势回答“何时发生”，两者不重复。
- 多日：保留每日堆叠柱，加入“其他/未细分”第五段，并以 `total_tokens` 控制每根柱的总高与 tooltip 总量。
- 真正空数据：只有权威总量为零时显示 no-data。
- 不一致：已知通道之和大于权威总量时显示 data-quality 降级，禁止负的“其他”段。

## 3. 不需要做的工作

- 不需要新增 `/api/trends_daily` 或新的数据库扫描；现有端点已提供范围、时区、通道和总量。
- 不需要重写 `/api/sessions` 的聚合/排序算法；只需把已经查询出的首末时间纳入兼容 DTO。
- 不需要从 transcript 中提取自然语言标题，这会把一个展示问题扩大到解析器、隐私和历史重建。
- 不需要引入图表库；现有 SVG/CSS/ES module 已能完成日柱，Data Bars 也有既有视觉契约。

## 4. 验证缺口

- `scripts/tests/dashboard-render-lifecycle.test.mjs` 只验证 `7d` 的 daily chart 包含四个 series，没有断言默认 `1d` 应显示数据，也没有覆盖 total-only/未细分通道。
- 现有 Node 测试未导入/验证 `render/top-sessions.js` 的标签、ID 隐藏、条长或三排序重绘。
- `src/web/mod.rs:4808-4839` 的 daily API 测试种子包含 reasoning 与 total，但只断言 date/event_count/cost，没有锁定 output/total 的显示契约。
- `tests/web_sessions_endpoint.rs:130-187` 覆盖三排序和过滤，但没有首末时间字段与旧响应兼容断言。
- 当前验收需要补充浏览器截图；自动 DOM 测试不能替代 light/dark、zh/en、宽屏/窄屏的可视比较证据。

## 5. 约束

- `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md:689-742` 继续约束完整筛选、稳定排序、shared secondary lifecycle、400 ms/128 KiB 预算与 Node/Rust 测试。
- `DESIGN.md:198,258-263,292-329` 继续约束 mono 数据、Data Bars、宽屏利用和紧凑真实空态。
- 当前工作树在任务创建前为 clean；本阶段只修改 `.trellis/tasks/08-23-usage-overview-session-composition-redesign/` 规划文件，未改产品代码，未运行 `task.py start`。

