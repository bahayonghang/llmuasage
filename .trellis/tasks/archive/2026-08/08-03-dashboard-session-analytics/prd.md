# PRD：会话分析 —— Top Sessions / 事件日志 / 小时热力图 / CSV 导出

父任务：`.trellis/tasks/08-03-dashboard-agentsview-alignment`。前置依赖：`08-03-dashboard-visual-system`（token 契约）、`08-03-dashboard-ready-widgets`（加载生命周期扩容模式）、`08-03-dashboard-timezone-iana`（IANA 时区契约，小时热力图必需）。真实契约事实：父任务 `research/review-verification.md`（§1 §2 §5 §7 §8 为本任务权威依据）。

## Goal

补齐 AgentsView 中依赖会话/时段维度的功能。本任务包含全部新后端工作：一个新查询 + 两个新路由 + 一个既有查询的契约扩展。

## Requirements

### R1 Top Sessions（新查询 `Dashboard::top_sessions` + 新路由 `GET /api/sessions`）

**不复用 `load_session_report`**（review-verification §1：它吃 `ReportFilter`——无 model/project_hash 过滤、绕过 Dashboard 门面、逐事件内存扫描）。新建：

- `src/query/top_sessions.rs`：SQL 聚合 `usage_event` GROUP BY session_id，接受完整 `QueryFilter`（source/model/since/until/project_hash/timezone），返回行含 session_id、session_label、project 标签、source（跨源会话置 None）、total_tokens、output_tokens、cost、span_minutes、active_minutes、event_count。
- 排序在 SQL 侧：`sort=tokens|duration|cost`（duration = active_minutes），**稳定排序**：排序键 DESC + session_id ASC tiebreak；`LIMIT`（默认 10，上限 50）。
- 错误映射：查询失败 → 该 section 降级 payload（对齐现有分节降级词汇），不 500。
- 路由仅挂 loopback；性能纳入 400ms/128KiB 预算（§7）。
- 前端：AgentsView TopSessions 规格 flex 列表（排名 mono / 标签+项目副行 / 指标 mono 蓝右对齐）+ `.seg` 三态排序（服务端排序，切换重新 fetch）；Duration 显示 active 分钟 +（span 分钟）。

### R2 事件日志查看器（`LogsQuery` 契约扩展 + UI）

现状（§2）：`LogsQuery` 无 session 过滤，`include_raw_json` 整页开关。扩展：

- 服务端 `session` 过滤参数：匹配 session_id 精确或 session_label 子串（大小写不敏感），SQL 侧 WHERE。
- 单记录详情契约：`event_key=<key>` 参数 → 返回恰一条记录且含 raw_json（替代整页 raw）；与分页参数互斥，互斥规则写进 rustdoc。
- 前端：新锚点 section `#logs`：分页表格（时间、源徽章、模型、会话标签、token 拆分、单事件成本、项目）；游标"加载更多"页大小 50；全局过滤变更重置；行展开按 `event_key` 拉取 raw。
- Top Sessions 行点击 → session_id 写入日志 `session` 过滤并跳转 `#logs`（服务端过滤，无"漏掉最近 50 条外目标"问题）。
- 性能：分页查询 < 30ms/页（`docs/prd/llmusage-integration-prd-v1.1.md:651` 预算）。

### R3 7×24 小时热力图（新查询 + 新路由 `GET /api/hour_of_week`）

- 后端 `src/query/hour_of_week.rs`：按 `hour_start` SUM 聚合 `usage_bucket_30m` 后，**Rust 侧**逐行经 `ResolvedZone` 转换取 (dow, hour)（§5：无 dow/hour SQL 表达式，一年 ≤ 17520 行 Rust 折叠开销可忽略；DST 语义自动继承 `timezone.rs`）。
- 返回 7×24 零填充网格 `{dow, hour, tokens, events}`；dow 约定 Monday=0，rustdoc 写明，前端负责周日置首重映射。
- 时区：依赖 timezone-iana 任务 —— 浏览器 IANA 名经 `ReportTimezone::Iana` 生效；标题旁显示当前时区名。
- 路由仅 loopback；进 Dashboard 门面（信号量/超时包装）。
- 前端：AgentsView 规格（CELL 17/gap 2/rx 2、行首周日、稀疏小时标签、`--hm-l*` 标尺、客户端 max×25/50/75% 分档、`.chart-tooltip`）；与贡献日历同 `.wide` 卡组合（分隔线 + 本图）。
- 点击下钻本期不做（llmusage 过滤模型无 dow/hour 维度）——PRD 明确此裁剪，仅 tooltip。

### R4 CSV 导出

- 顶栏 Export CSV：从已加载快照生成多段 CSV（summary 六卡、daily trends、projects、models、sources、top sessions）。
- **公式注入防护必选**（§8，对齐 `ref/repo/agentsview/frontend/src/lib/utils/csv-export.ts:19`）：`/^[=+\-@\t\r\n]/` 命中前缀单引号，再做标准逗号/引号/换行转义。项目名、模型名、会话标签一律视为不可信输入。
- BOM 头（Excel 中文兼容）；文件名 `llmusage-analytics-YYYYMMDD.csv`；zh/en 表头随当前语言。
- **纯函数测试必选**（非"若可行"）：转义/注入防护/多段拼装各至少一条 node 用例。

### 通用

- 新面板并入 `SECONDARY_SECTIONS` 生命周期（沿用 ready-widgets 建立的扩容模式 + 测试模式）。
- 新端点仅 loopback；public 投影不变（两个新端点 public 下 404 有测试）。
- 新字符串 zh/en；快照模式：Top Sessions / 小时热力图进快照 DTO 扩展（沿用 ready-widgets 的 Option 字段模式），日志查看器快照下显示"仅 live 可用"提示。
- 新 JS 登记 `ASSET_MANIFEST`（29 → 33）。

## 非目标

- 转录/消息正文相关一切；独立会话详情视图；`blocks_report`/`context_pressure`（另立任务）；小时热力图点击下钻。

## Acceptance Criteria

- [ ] `/api/sessions`、`/api/hour_of_week` Rust 集成测试：空库、source/model/project 过滤、三排序稳定性（同值 tiebreak 断言）、limit 上限、**public 404**、时区（Asia/Shanghai vs UTC 偏移 8 小时一例）。
- [ ] `LogsQuery` 扩展测试：session 精确/子串匹配、event_key 单记录含 raw、与分页互斥行为。
- [ ] **性能**（§7）：sessions/hour_of_week 热身后 ≤ 400ms（代表性本地库实测记录）；logs 分页 < 30ms/页；载荷 ≤ 128 KiB。
- [ ] Top Sessions 数值与 SQL 手工聚合抽查一致；行点击→日志过滤联动正确。
- [ ] 小时热力图与 SQL+时区手工折算抽查 2 格一致。
- [ ] CSV：node 纯函数测试通过（含 `=cmd()` 型注入样例）；Excel 打开中文无乱码。
- [ ] latest-wins：新面板 stale 丢弃 node 用例通过。
- [ ] `just ci` 全绿；docs 双语更新；新端点补进 `docs/prd/llmusage-integration-prd-v1.1.md` 端点清单（若其为权威）。
