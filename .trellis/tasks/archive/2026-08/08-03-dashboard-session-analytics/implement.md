# Implement：会话分析

前置阅读：本任务 `prd.md`、`design.md`；父任务 `research/review-verification.md`。前置任务 `08-03-dashboard-visual-system`、`08-03-dashboard-ready-widgets`、`08-03-dashboard-timezone-iana` 均已合入。

> ⚠️ JS/CSS 走 Bash；Rust 用 Edit，提交前 `cargo fmt`。

## 步骤

### 0. 剩余契约核对（仅实现级细节，宏观契约已在 review-verification 固化）

- [ ] 读 `logs.rs`/`reports.rs` 的 session_id 取值语义（空值/合成规则），写进 top_sessions 设计注释。
- [ ] 核对 `usage_bucket_30m.hour_start` 单位（一行样本）。
- [ ] 结果存本任务 `research/contracts.md`。

### 1. 后端 `Dashboard::top_sessions` + `/api/sessions`

- [ ] `src/query/top_sessions.rs`（SQL 聚合 + 稳定排序 + duration 全候选批量精算路径）+ handler（信号量/超时/降级映射）。
- [ ] `tests/web_sessions_endpoint.rs`：空库、source/model/project 过滤、三排序稳定性（同值 tiebreak）、limit 钳制、public 404。
- 验证：`cargo test web_sessions -- --test-threads=1`；`cargo clippy`。

### 2. 后端 `LogsQuery` 扩展

- [ ] `session` 过滤 + `event_key` 单记录模式（互斥规则 rustdoc）+ 参数解析。
- [ ] logs 测试扩展：精确/子串匹配、event_key 含 raw、互斥行为、既有分页回归。
- 验证：logs 相关测试 + 分页 30ms 预算复测（代表库计时记录）。

### 3. 后端 `hour_of_week` + 路由

- [ ] `src/query/hour_of_week.rs`（SQL 粗聚合 + Rust ResolvedZone 折叠 + 零填充 + rustdoc dow/桶归属约定）+ handler。
- [ ] `tests/hour_of_week.rs`：空库、过滤、Asia/Shanghai vs UTC 偏移 8 小时断言、public 404。
- 验证：`cargo test hour_of_week -- --test-threads=1`。

### 4. 前端 TopSessions

- [ ] `render/top-sessions.js` + fetch 扩展 + `SECONDARY_SECTIONS` +1（node 断言同步）+ shell.rs 容器 + copy 键 + manifest +1。
- [ ] 行点击联动留接口（步骤 5 接通）。
- 验证：三排序网络请求正确；空库空态；node 测试。

### 5. 前端 LogsViewer

- [ ] `#logs` section + `render/logs-viewer.js`（懒加载首页、游标累加、过滤签名重置、event_key 行展开、快照 live-only 提示）+ copy 键 + manifest +1；接通步骤 4 行点击。
- 验证：分页 3 页连贯、session 联动过滤、raw 展开缓存、全局过滤变更重置。

### 6. 前端 HourOfWeek

- [ ] `render/hour-of-week.js`（几何、Sun 置首 remap、客户端分档、tooltip）挂贡献日历同卡 + `SECONDARY_SECTIONS` +1 + copy 键 + manifest +1。
- 验证：与 SQL+时区手工折算抽查 2 格；浏览器时区（IANA 参数）生效验证一例。

### 7. CSV 导出

- [ ] `assets/csv-export.js`：`escapeCsvCell`（公式注入防护）+ 多段拼装纯函数 + 下载壳 + 顶栏按钮 + copy 键 + manifest +1（终态 `[WebAsset; 33]`）。
- [ ] **必选** node 用例：注入样例（`=cmd()`/`+1`/`-1`/`@x`/`\t` 开头）、引号换行转义、多段结构、zh/en 表头。
- 验证：Excel 打开中文无乱码、注入单元格显示为文本。

### 8. 快照 + 性能 + 收尾

- [ ] 快照 DTO 加 top_sessions/hour_of_week 两键（Option 模式）+ 前端快照路径 + 旧快照兼容用例。
- [ ] `curl -w` 实测两个新端点（≤ 400ms / ≤ 128 KiB）+ duration 精算计时（< 100ms）记录进 research。
- [ ] docs 双语更新；新端点补进 `docs/prd/llmusage-integration-prd-v1.1.md` 端点清单。
- [ ] `just ci` 全绿。

## 回滚点

后端三步（1/2/3）各一提交；前端四步（4/5/6/7）各一提交；快照步骤随 8 单独提交。联动点在步骤 5 才接通，任一 revert 不影响其余。

## 提交建议

`feat(看板): [AI] ✨ 新增 /api/sessions Top Sessions 查询与排名` 等，每步一条。
