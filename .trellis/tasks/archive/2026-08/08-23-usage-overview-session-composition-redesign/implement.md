# Implement：会话消耗图与范围感知 Token 构成

## Completion record

Implementation and independent Trellis review are complete. The checklist below
is retained as the approved execution plan; the final, evidence-backed gate
status is recorded in `evidence/verification.md`.

## Preconditions

- [ ] 用户已审阅并明确批准最新 `prd.md`、`design.md` 和本清单；批准前不得运行 `task.py start` 或修改产品代码。
- [ ] 实施前运行 `trellis-before-dev`，读取 `.trellis/spec/guides/{cross-layer,code-reuse}-thinking-guide.md`、`.trellis/spec/llmusage/backend/{dashboard-performance,token-accounting}-contracts.md` 与 `DESIGN.md` 相关条款。
- [ ] 检查工作树并保留无关用户改动；本任务不得顺带修改解析器、schema、CSV 或其他 dashboard 模块。

## 1. Lock the existing contracts with focused tests

- [ ] 在 `tests/web_sessions_endpoint.rs` 为现有三排序、canonical id 和日志下钻 fixture 补充首末事件时间预期，先证明新增字段可从现有 SQL 无额外扫描取得。
- [ ] 在 `src/web/mod.rs` 的 daily API 测试补 `input/cache/cache creation/output/total` 逐字段断言，确认 output 不普遍加 diagnostic reasoning、total 仍是权威值。
- [ ] 在 `scripts/tests/dashboard-render-lifecycle.test.mjs` 增加当前 `1d` 空态回归用例作为 red test，并加入 total-only、known < total、known > total fixtures。
- [ ] 导入 `top-sessions.js` 所需 DOM/window stub，先写 ID 不进入 markup/accessible text、metric ratio 和旧 snapshot fallback 的 red tests。

**Gate A — red evidence**

- [ ] 聚焦用例以预期原因失败；既有不相关用例保持通过。记录失败名称，不以全文 DOM 快照替代语义断言。

## 2. Add session time context without changing query semantics

- [ ] 在 `TopSessionRow` 增加 `first_event_at` / `last_event_at`，从现有 candidate SQL 列赋值；复用同一字段计算 span/active，避免重复字符串或第二次查询。
- [ ] 更新 Rust tests：空库、三排序、过滤、duration 全候选、JSON payload 与 snapshot 都含准确时间；stable tiebreak 和 limit 不变。
- [ ] 确认 `/api/sessions` public 404、loopback support/degraded payload 和 Logs canonical matching 不回归。

**Gate B — backend contract**

- [ ] `rtk cargo test --test web_sessions_endpoint -- --test-threads=1`
- [ ] 相关 `src/web/mod.rs` API tests 通过。

## 3. Extract and reuse the source display catalog

- [ ] 新建 `src/web/assets/data/source-catalog.js`，迁移 catalog validation、fallback display name、parse/cache/display lookup；不复制注册来源列表。
- [ ] `hero.js` 改为消费新模块并兼容转发现有测试用导出；Agent 徽章 DOM/顺序/Logo/fallback 行为逐字保持。
- [ ] `top-sessions.js` 只读取 `sourceDisplayName(source)`，未知来源安全 title-case fallback。
- [ ] 将新模块登记到 `src/web/assets/mod.rs`，同步 manifest 长度/唯一性断言和 static export。

**Gate C — reuse without Hero regression**

- [ ] 现有 agent badge catalog/markup tests 与 Web asset tests 全部通过。

## 4. Implement the session consumption bar chart

- [ ] 抽出可测试纯函数：当前 metric 数值/格式、max ratio、项目/Agent/时间 label、旧 snapshot fallback、完整 accessible name。
- [ ] 将 list markup 替换为十条 native button bar rows；canonical id 只放内部 dataset，禁止进入可见/tooltip/ARIA 文本。
- [ ] 排序点击在 await 前更新选中/忙碌状态；保留上次成功 bars；增加 catch/finally、局部 degraded 提示和 generation/reload 双重 stale fence。
- [ ] 更新 `copy.js` ZH/EN：新标题、副文案、无项目回退、时间/event/active fallback、排序 loading/error、日志下钻提示。
- [ ] 在 `components.css` 实现 track/fill、label/value grid、focus/hover/disabled/degraded 和 `<=720px` 两行布局；不加 shadow、渐变、动画或新依赖。

**Gate D — session chart behavior**

- [ ] Node tests 覆盖 tokens/duration/cost ratio、全零、极端值、长/重复/无项目标签、无时间旧快照、ID 不泄漏、click-to-logs、sort success/failure/stale。
- [ ] `rtk node --check src/web/assets/render/top-sessions.js`

## 5. Implement one authoritative Token composition model

- [ ] 在 `trends-daily.js` 增加纯 derivation：数值规范化、known sum、权威 total、other residual、inconsistent detection、1d rows aggregate。
- [ ] 通道固定为 input/cache read/cache creation/output/other；不得读取或相加 `reasoning_output_tokens`。
- [ ] 修正 `DailyTrendPoint` rustdoc 中“output 已含 reasoning”的过时表述，并用 API test 固化真实字段语义；不修改 token SQL 或 parser。
- [ ] 只有总量为零且通道一致时返回 no-data；负数、非有限值或 known > total 返回结构化 renderer degraded reason。

**Gate E — accounting correctness**

- [ ] Node pure tests 覆盖 exact sum、total-only、reasoning-like residual、跨午夜两行聚合、zero、known > total、negative/non-finite。
- [ ] Rust daily API contract test 与 `.trellis/spec/llmusage/backend/token-accounting-contracts.md` 一致。

## 6. Render 1d composition and five-part daily bars

- [ ] `1d`/same-day 渲染 100% horizontal strip + 总量 + 五项精确 Token/占比；删除当前无条件 `copy.oneDay` 早退。
- [ ] 多日 SVG 用 `max(row.total_tokens)` 定标，渲染第五段 other；tooltip 包含权威总量、五段与成本。
- [ ] `copy.js` 增加范围说明、other、inconsistent 与四类状态 ZH/EN；旧 `oneDay` 文案删除或确认无引用后清理。
- [ ] `charts.css` 增加第五类语义 class、1d strip/stats、窄屏重排；保证颜色不是唯一通道标识。
- [ ] loading/degraded/no-data/inconsistent 分支都保留 header 并提供准确原因。

**Gate F — composition rendering**

- [ ] Node render tests覆盖 `1d` 有数据不为空、1d zero、多日五段、tooltip/legend、双语、旧 snapshot/no-key 和不一致降级。
- [ ] `rtk node --check src/web/assets/render/trends-daily.js`

## 7. Documentation and design contract

- [ ] 更新 `docs/dashboard/index.md` 与 `docs/zh/dashboard/index.md`：会话条形图标签/下钻、1d 聚合构成、多日每日构成、其他/未细分和权威总量语义。
- [ ] 定点更新 `DESIGN.md` 的 Data Bars/ready-widget 合同，记录会话图和范围感知构成；不处理无关历史视觉方向。
- [ ] 明确 analytics CSV 未改，避免文档暗示 UI 去 ID 等于机器导出字段变化。

## 8. Automated validation

- [ ] `rtk node --test scripts/tests/dashboard-render-lifecycle.test.mjs`
- [ ] `rtk node --test scripts/tests/dashboard-fetch.test.mjs`
- [ ] 运行新增/相关 Top Sessions Node tests（若并入 lifecycle，记录 test name filter）。
- [ ] `rtk cargo test --test web_sessions_endpoint -- --test-threads=1`
- [ ] 运行相关 Web API/snapshot tests。
- [ ] `rtk cargo fmt --check`
- [ ] `rtk cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `rtk git diff --check`

## 9. Performance and bounded visual verification

- [ ] 用代表性只读/备份数据库热身后测 `/api/sessions` 三种 sort，各至少 5 次，记录 p95 与最大 payload；门槛 `<=400 ms` / `<=128 KiB`。
- [ ] 确认 `/api/trends_daily` 未新增扫描/字段；对同一 filter 比较实现前后 JSON 请求数与 query route。
- [ ] 用同一 fixture 截图 1440px、1920px 的 light/dark × zh/en，并截一个 `<=720px` 窄屏。
- [ ] 第一轮批量检查：条长比较、技术 ID 不可见、重复项目可区分、排序反馈、五类构成、1d 不空、长标签、焦点、对比度、窄屏无页面溢出。
- [ ] 批量修复后最多一次确认轮；未取得的浏览器/人工证据标记 `UNVERIFIED`，不得用自动测试替代。

## 10. Full gate and finish

- [ ] `rtk just ci`
- [ ] 使用 `trellis-check` 复核 PRD/Design/实现、跨层数据流、测试和视觉证据。
- [ ] 若实施揭示可复用契约缺口，通过 `trellis-update-spec` 更新 dashboard/token spec；不要把本任务细节堆入全局规范。
- [ ] 最终 diff 只含本任务相关代码、测试、双语文档、定点 `DESIGN.md` 与任务文件。
- [ ] 按仓库规范生成窄范围中文 emoji Conventional Commit；不 push、不建 PR，除非用户另行授权。

## Rollback points

1. `TopSessionRow` additive time fields + Rust tests；可独立 revert，无 schema 迁移。
2. source catalog extraction；必须连同 Hero compatibility tests 一起 revert。
3. session chart renderer/copy/CSS；日志 API 和 canonical id 不受影响。
4. Token composition renderer/copy/CSS/rustdoc；`/api/trends_daily` 和数据库不受影响。
