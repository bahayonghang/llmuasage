# Design：会话消耗图与范围感知 Token 构成

## 1. Design intent and boundaries

该页面属于 **Operate** 模式。用户需要快速识别消耗热点、比较数量级并下钻到日志，而不是阅读内部 session identity。本任务是既有“用量概览”的局部信息架构和渲染重设计：保留当前筛选、查询、排序、日志和视觉系统，只调整两个面板及必要的 additive DTO。

```text
QueryFilter
  ├─ /api/sessions -> TopSessionRow + first/last event time
  │                    -> human context + metric bars -> exact Logs session filter
  └─ /api/trends_daily -> authoritative total + four visible channels
                         -> other/unclassified residual
                         -> 1d composition strip | multi-day stacked bars
```

直接实现边界：

- `src/query/top_sessions.rs`：给现有响应增加首末事件时间，不改变聚合、排序和 canonical identity。
- `src/web/assets/render/top-sessions.js`：横向会话消耗条形图、标签上下文和排序反馈。
- `src/web/assets/render/trends-daily.js`：统一构成派生、1d 聚合图和多日堆叠柱。
- `src/web/assets/data/source-catalog.js`（新）：抽取 Hero 已有的来源目录解析与展示名查询，供 Hero 和会话图共享。
- `src/web/assets/{charts,components}.css`、`copy.js`：两块图表的视觉、状态、响应式和双语。
- Rust/Node tests、双语 dashboard docs 与 `DESIGN.md` 的定点契约更新。

不改变解析器、数据库 schema、token 归一化、定价、CSV 列或日志查询契约。

## 2. Session ranking data contract

### 2.1 Additive response fields

`TopSessionRow` 增加：

```rust
pub first_event_at: String, // filtered session MIN(event_at), RFC3339 UTC
pub last_event_at: String,  // filtered session MAX(event_at), RFC3339 UTC
```

- SQL 已选择 `MIN(e.event_at)` / `MAX(e.event_at)`，当前 `Candidate` 也已持有两值；实现只把它们带入序列化 row，不增加扫描或查询。
- 时间描述必须说“当前筛选范围内首次/最近事件”，不暗示完整会话生命周期。
- `span_minutes` 与 `active_minutes` 继续由现有首末时间和 30 分钟 gap cap 精算；三种排序及 `session_id` tiebreak 完全不变。
- live 新响应总有两个字段；旧 snapshot 缺失时前端用现有 active/span/event_count 生成无 ID 回退。

### 2.2 Internal identity boundary

- canonical `session_id` 继续存在于 JSON 和 DOM `data-session-id`，仅供点击/键盘激活后发出 `llmusage:session-select`。
- canonical id、`session_label`、source file stem、UUID、hash 不进入可见标签、`title`、tooltip 或可访问名称。
- 不在前端维护 UUID/哈希正则黑名单；该策略会漏掉未来 ID 形状，也会误判真实标题。

## 3. Shared source display catalog

最新 Hero 已通过 `#source-badge-catalog` 从注册表向 live/snapshot shell 提供 `{id, display_name, logo_url}`。现在出现第二个展示名消费者，应把目录解析从 `render/hero.js` 抽到 `data/source-catalog.js`：

```text
registered_source_descriptors()
  -> shell application/json catalog
  -> source-catalog.js validate/cache
     ├─ hero.js badge list
     └─ top-sessions.js Agent display name
```

- 保留 `hero.js` 现有导出作为兼容转发，避免 Node 测试和潜在模块消费者被无关破坏。
- 新模块缓存按目录 script 文本建立的 map；unknown source 使用已有稳定 ID 的安全 title-case fallback。
- 只复用展示名，不把 Agent Logo 塞进排行条，避免十行高彩 Logo 干扰数值比较。
- 新 ES module 登记 `src/web/assets/mod.rs`，live 与 snapshot/export 同步可用。

## 4. Session consumption chart

### 4.1 Information hierarchy

- 面板名改为“会话消耗排行” / “Session consumption ranking”。副文案说明“按当前指标排序；选择一项查看事件日志”。
- 保留三态 `.seg`：Token 用量、活跃时长、估算成本。
- 每条结构：排名 → 人类上下文 → 水平 metric track/fill → 精确值。
- 主标签：`project_label`；缺失时为本地化 `{Agent} 会话`。
- 副标签：
  - 有时间：`{Agent} · {first local}–{last local}`；单事件只显示一个时间；
  - 旧快照无时间：`{Agent} · {event_count} events · active {minutes}`；
  - 不显示 technical session label。

### 4.2 Metric encoding

```text
metricValue(row, sort):
  tokens   -> total_tokens
  duration -> active_minutes
  cost     -> cost_usd

ratio = maxMetric > 0 ? metricValue / maxMetric : 0
```

- 第一名填满 track；其他条按同一线性尺度显示。零值不伪造最小长度，精确值仍可见。
- fill 使用既有 accent/data-bar token；排名、条长和右侧数字共同表达，不靠颜色。
- 成本值保留 `$0.00` 精度；时长显示 active，必要时辅助文案保留 span，但条长只编码 active。
- 十条仍是现有 limit；不增加 show-more，避免与“Top 10 快速定位”目标漂移。

### 4.3 Interaction states

- 点击排序时立即更新 `aria-pressed`、`aria-busy` 和轻量 refreshing 文案，保留上次成功的 bars；不先清空面板。
- `requestGeneration + dashboard reloadGeneration` 继续拒绝 stale response；sort fetch 增加显式 `try/catch/finally`，失败时保留旧 rows 并显示局部 degraded 提示，不产生 unhandled rejection。
- 整行仍为 `<button>`，accessible name 包含项目/Agent/时间、当前指标值和“查看事件日志”；内部 bar 标记为装饰，避免屏幕阅读器重复朗读。
- Enter/Space 原生触发下钻；focus-visible 使用现有 accent ring。

## 5. Authoritative Token composition

### 5.1 One shared derivation

在 `trends-daily.js` 暴露可测试纯函数：

```text
known = input + cache_read + cache_creation + output
total = total_tokens

if any channel/total is non-finite or negative -> inconsistent
if known > total -> inconsistent
other = total - known
segments = [input, cache_read, cache_creation, output, other]
```

- `total_tokens` 是柱高、占比和 tooltip 总量的唯一权威分母。
- “其他/未细分”覆盖 total-only 来源、独立 reasoning 或来源未细分的差额；它不等同于 reasoning，也不能命名为 reasoning。
- 不修改 SQL 把 `reasoning_output_tokens` 普遍加到 output。相反，修正 `DailyTrendPoint` 过时的“output 已含 reasoning”注释，使其与 token-accounting 契约和真实查询一致。
- 任一行 `known > total` 时，面板进入 data-quality degraded，显示日期和差额概述；不 clamp、不画负段、不静默用 known 改写 total。

### 5.2 1d / same-day composition

- 将返回的 1–2 个本地日片段按五个 segment、`total_tokens` 和 cost 聚合；“近 24 小时”跨本地午夜时仍得到一份完整范围构成。
- 主要图形是一条 100% stacked horizontal strip；下方最多五个紧凑统计项显示颜色/纹理标记、名称、精确 Token 与占比。
- 总量和范围说明始终可见；当总量为零才显示真正 no-data。
- 该模式不画小时趋势，因为上方短时趋势已经回答“何时发生”，本面板只回答“由什么构成”。

### 5.3 Multi-day composition

- 保留现有每日 SVG 堆叠柱，扩展为五段；每根柱总高以该日 `total_tokens / maxDailyTotal` 计算，五段内部按绝对值堆叠。
- y 轴使用 `max(total_tokens)` 的 niceScale；tooltip 显示权威总量、五段数值和成本。
- 图例增加“其他/未细分”；通道顺序在 1d、多日、ZH/EN 和 tooltip 中完全一致。
- 横向滚动和月初/首末日期标签规则保留；空、loading、degraded 与 inconsistent 使用不同 copy。

## 6. Visual, responsive, and accessibility contract

- 会话排行使用 `DESIGN.md` 既有 Data Bars；Token 1d strip 与多日柱继续使用 `--chart-cat-*`，第五类新增语义 token/class，但不新增品牌色。
- 第五类用中性/纹理或低饱和 token，并始终有文字图例；四类现有颜色不可成为唯一识别方式。
- 桌面会话条使用 label/value 固定区 + 可伸缩 track；`<=720px` 改为两行 grid（标签/值第一行，track 第二行），无页面级横向溢出。
- 长项目名视觉截断，但按钮 accessible name 保留完整项目名；时间使用当前 `QueryFilter.timezone`/浏览器 IANA 时区格式化。
- pointer tooltip 只补充细节；构成统计、精确值、排序状态和下钻均可通过键盘/静态文本获得。
- light/dark × zh/en 共用 DOM；所有新增文案进入 `copy.js`。

## 7. Compatibility, performance, and rollback

- API 是 additive field change；旧客户端忽略时间字段，旧 snapshot 在新前端走 fallback。
- `/api/trends_daily` 形状不变；“其他”完全是派生，不增加后端扫描、序列化字段或缓存 key。
- `/api/sessions` 载荷只多两个短字符串/row，仍需实测 `<=400 ms`、`<=128 KiB`；排序 fetch 继续使用现有 10 秒/32 entry cache 与 abort 支持。
- 回滚可以分别撤销：
  1. 会话 DTO + 条形图恢复旧 list；日志和数据不迁移；
  2. Token renderer 恢复四段 daily chart；API/数据库不迁移。
- 不改变 public router：`/api/sessions` 仍仅 loopback；snapshot 仍完整兼容。

## 8. Requirement-to-mechanism map

| Requirement | Design mechanism | Primary evidence |
| --- | --- | --- |
| R1 | additive time fields + internal-only ID + horizontal metric bars | Rust row/API tests + Node markup/metric tests + screenshots |
| R2 | authoritative composition derivation + 1d strip + five-part daily bars | Node pure-function/render tests + daily API contract test |
| R3 | shared secondary lifecycle, existing endpoints, old snapshot fallback | fetch/load lifecycle tests + snapshot fixture |
| R4 | existing tokens, responsive grid, native buttons/ARIA, bilingual copy | DOM tests + bounded visual QA |

## 9. Risks and trade-offs

1. 项目名会在多个会话中重复。时间范围和排名提供区分，换来保留具体日志下钻；用户已确认这一取舍。
2. “其他/未细分”不是单一来源通道。它诚实表达权威总量无法由四个可见通道完全解释，比错误并入 output 或让柱子变矮更可靠。
3. 抽取 source catalog 会触碰刚完成的 Hero；保留原导出、先跑现有 agent badge Node tests，限制为机械复用。
4. 无法用自动化证明最终图表扫读体验；浏览器截图和人工检查仍是必选证据，缺失时标记 `UNVERIFIED`。

