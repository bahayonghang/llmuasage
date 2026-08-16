# Design：zcode 被动解析器

依据：父任务 `research/zcode-artifacts.md`（本机 schema/语义证据）、`research/ccusage-tokscale-reference.md` §3（tokscale 参考）、`src/parsers/opencode.rs`（高水位模式范本）。

## 1. 边界与组件

```
src/domain/models.rs            SourceKind::Zcode
src/domain/source_descriptor.rs SOURCE_DESCRIPTORS += zcode（Precise / LocalDatabase / parser）
src/domain/platform_monitor.rs  PLATFORM_MONITORS += zcode（root ~/.zcode，env ZCODE_HOME）
src/registry.rs                 registered_parsers() += ZcodeParser
src/integrations/zcode.rs       resolve_db_path()（新，env+默认路径，不写任何文件）
src/parsers/zcode.rs            ZcodeParser + sync_zcode + row→UsageEvent 归一化
src/store/cursor.rs             zcode 高水位 cursor 读写（对齐 opencode cursor 表用法）
tests/sync_regression.rs        seed_zcode + 集成测试组
```

不新增 Cargo 依赖（rusqlite/chrono/serde_json 已有）。

## 2. 数据流（对齐 opencode 步骤注释风格）

1. `resolve_db_path()`：`ZCODE_HOME` 存在则用 `<ZCODE_HOME>/cli/db/db.sqlite`，否则 `~/.zcode/cli/db/db.sqlite`。**`ZCODE_HOME` 是 llmusage 自造覆盖名**（zcode 官方无公开根变量；第三方用 `ZCODE_STORAGE_DIR`/`ZCODE_DB`，见 research §5）——默认路径才是契约，env 仅用于测试/重定向。缺失 → 写 `passive_no_data` 状态、返回空 stats（absent 语义对齐 kimi missing-root 测试）。
2. 只读连接 `file:...?mode=ro`；`pragma table_info(model_usage)` 探测 `computed_total_tokens`（tokscale 同款防御，避免 query 失败推断）。
3. 读 cursor：`last_completed_at`（毫秒）+ `last_processed_ids`（同毫秒行 id 集合，锚点）。
4. 分页 SELECT（页大小对齐 `OPENCODE_PAGE_SIZE=1000`）——**水位用 `completed_at` 而非 `started_at`**（修订：started_at 水位会永久漏掉延迟完成的请求——A 开跑未完、B 完成推进水位、A 随后完成时其 started_at 已在水位之下；只有 completed 行才有 completed_at，用水位 + status 过滤天然闭合该窗口）：

```sql
SELECT id, session_id, turn_id, model_id, provider_id, query_source,
       started_at, completed_at, status, finish_reason,
       input_tokens, output_tokens, reasoning_tokens,
       cache_creation_input_tokens, cache_read_input_tokens,
       computed_total_tokens, provider_total_tokens
  FROM model_usage
 WHERE status = 'completed'
   AND ( completed_at > :watermark
      OR (completed_at = :watermark AND id NOT IN (:anchors…)) )
 ORDER BY completed_at, id
 LIMIT :page
```

   - **括号**：status 过滤在最外层 AND，范围条件整体加括号——初稿 `a > ? OR (...) AND status=...` 因 AND 优先级会放行较新的 error/cancelled 行。
   - **动态 projection**：探测到 `computed_total_tokens` 列缺失（旧 schema）时切换到变体 B：`CAST(NULL AS INTEGER) AS computed_total_tokens` + 代码层降级 `provider_total_tokens` → 通道求和。初稿"始终 SELECT 该列"会让旧 schema 在查询阶段直接失败、来不及 fallback。
   - **skipped/issues 统计**（error/cancelled 行数进 stats）：独立的聚合查询 `SELECT status, COUNT(*) FROM model_usage WHERE completed_at > :watermark GROUP BY status`，不与事件 SELECT 混用（SQL 层过滤后事件查询看不到这些行）。
5. 每行 → `UsageEvent`；满页 `writer.commit_shard(SyncShard)`；页后保存高水位（事务内）。
6. 锚点校验：页 0 查询结果不包含任何已知锚点 id → DB 被重建 → 重置 cursor 全量重放（opencode `opencode_cursor_anchor_exists` 模式）。

### 2b. bounded run（--recent-days）契约

- `recent_cutoff` 有值时：事件查询以 `completed_at >= cutoff 毫秒` 过滤，**以已存水位为下界但不得推进水位/锚点**、不执行任何 reset（契约 `source-sync-contracts.md:86-88`：bounded run 可复用全历史 cursor 作下界，但不得推进它；随后全量 sync 必须仍能恢复窗口外历史）。opencode 的 `page_last_time = cutoff.max(cursor.last_time_created)` 同款手法。
- 行更新语义（PRD"行更新"测试的算法）：行以 `id` 为主键、attempt 是新行（id 唯一实测），正常路径无原地改值；若未来出现同 id 行 token 变化（重试覆盖），靠全量 rebuild 兜底，不在增量路径处理——PRD 测试改为覆盖"DB 重建重放"与"追加行"两类，不承诺原地更新检测。

## 3. UsageEvent 构造

| 字段 | 来源 | 说明 |
| --- | --- | --- |
| `source` | 常量 | `zcode` |
| `event_key` | `zcode:<sha256(id)>` | `id` 是主键且全表唯一（0 重复实测）；**不拼 attempt_index**（id 与 logical_request_id/attempt_index 无拼接关系，见 research §0） |
| `event_at` | `started_at`（ms→RFC3339 UTC；报告语义锚点，tokscale 同款 start-anchored） | 注意与 cursor 水位（`completed_at`，可见性语义）区分 |
| `session` | `session_id` 哈希化（对齐 kimi/pi 的 session 处理） | |
| project 归属 | join `session` 取 `directory`/`path` 哈希 | **不落 `title`**；join 失败（session 行缺失）→ project 为空，不报错 |
| `model` | `normalize_model(model_id)`，fallback `zcode-unknown` | GLM-5.3 / deepseek-v4-flash 都走现有归一化 |
| tokens | 见下表 | |
| cost | `PricingStatus::Unpriced`（catalog 无条目，自动） | |

token 通道（依据 `.trellis/spec/llmusage/backend/token-accounting-contracts.md:29-34` + 本机 1070 行证据，全部 u64 饱和防负）：

| 通道 | 公式 | 依据 |
| --- | --- | --- |
| `input` | `input_tokens - cache_read_input_tokens - cache_creation_input_tokens`（饱和减） | 契约要求内部 input 为非缓存；zcode 上报 input 含 cache。cache_creation 本机全 0 但公式预留（codeburn 同款，外部佐证） |
| `cache_read` | `cache_read_input_tokens` 原列 | |
| `cache_creation` | `cache_creation_input_tokens` 原列 | |
| `output` | `output_tokens` **原样保留（含 reasoning）** | deepseek-v4-flash 473 行证据：reasoning ≤ output 且 total=in+out；若再减 reasoning 会与权威 total 对不上（不学 tokscale 的 output-reasoning 减法——其 `TokenBreakdown::total()` 是五桶求和，语义不同） |
| `reasoning` | `reasoning_tokens` | 诊断通道，**不计入 total**（契约默认） |
| `total` | `computed_total_tokens` 权威 | completed 行全表 = in+out |

- 一致性校验：`computed_total_tokens != input_tokens + output_tokens` 时 parse issue 计数一次（kimi extreme-value 同款），不改数；降级路径：无 `computed_total_tokens` 列（旧 schema）→ `provider_total_tokens` → 通道求和。
- `input_tokens < cache_read + cache_creation` 的病态行：clamp 到 0 + issue 计数。
- 全零行跳过（pi all-zero 语义）。

## 4. 游标存储

- 复用 opencode 的 cursor 存储形态（`store.cursors()` 侧新增 `load_zcode_cursor/save_zcode_cursor`，落在既有 cursor 表/机制；具体表结构跟随 `src/store/cursor.rs` 现状，不新开迁移除非必要——`source` 为 TEXT 自由列，usage 行无需迁移）。
- cursor 内容：`{last_completed_at: i64, last_processed_ids: Vec<String>, updated_at}`；DB fingerprint（size+mtime）可选存入用于重建检测，锚点校验为主。

## 5. 兼容与回滚

- 新来源无历史行：回滚 = 注销 parser + descriptor（数据残留无害，`source='zcode'` 行可被 `sync --rebuild` 清理策略处理——确认 rebuild 对新源的 sweep 行为与 grok 一致）。
- sync 命令：走通用 parser 路径，无需 parserless 特判（区别于旧 antigravity 分支）。
- Windows 路径：`~/.zcode` 解析复用现有 home 解析工具（kimi/pi 同款），无需平台分支。

## 6. 测试设计

- 单测（`src/parsers/zcode.rs` 内，tempfile + rusqlite 建合成 DB）：
  - completed/error/cancelled 混合 → 只导 completed，skipped 计数正确。
  - cache-inclusive 修正数值断言，覆盖两种模型形状：GLM 行（in=60543, cr=56960 → input=3583, reasoning=0）；deepseek 行（in=316, cr=256, out=391, reasoning=380, total=707 → input=60, output=391 原样含 reasoning, reasoning=380, total=707）。
  - 旧 schema（drop `computed_total_tokens` 列）降级。
  - 全零行 / 病态负值 / 超大值饱和。
- 集成（`tests/sync_regression.rs`，`seed_zcode` helper 造 DB）：
  - `zcode_sync_twice_is_idempotent`（第二次 changed/inserted=0）。
  - `zcode_append_imports_only_new_rows`。
  - `zcode_late_completing_request_is_not_missed`（修订新增）：A 行 started 早但初始 status=running/completed_at NULL → 先插 B 行（更晚 started+completed）并 sync 推进水位 → 再把 A 行置 completed（completed_at 更晚）→ 再 sync 必须导出 A。
  - `zcode_skips_error_and_cancelled_rows_and_counts_them`（统计走聚合查询）。
  - `zcode_db_rebuild_replays_from_zero`（删锚点行触发重放，无重复 event_key）。
  - `zcode_missing_root_sync_succeeds_and_reports_no_data`。
  - `zcode_home_override_points_parser_at_custom_root`（`ZCODE_HOME` env，Fixture 的 env save/restore 机制）。
  - `zcode_first_sync_marks_current_token_accounting`（对齐 kimi 同名测试）。
  - `zcode_recent_days_run_filters_window_without_advancing_cursor`（bounded：不推水位/锚点，窗口外行留给全量 sync 恢复）。

## 7. 权衡记录

- **SQLite vs rollout JSONL**：选 SQLite（通道更全、状态过滤、稳定主键、高水位游标天然）；rollout JSONL 含完整 prompt 且缺 reasoning 通道，双读会重复计数（tokscale 靠 dedup 兜底，llmusage 不做）。
- **status 过滤**：error/cancelled 行 token 实测全 0（research §0），排除不丢用量；如后续证明 cancelled 也计费，改 WHERE 条件即可（cursor 不受影响）。
- **output 不减 reasoning**：契约规定 reasoning 是诊断通道；本机数据证明 output 含 reasoning 且 total=in+out，减了反而破坏权威 total 对账。与 tokscale 的差异是刻意的（两边 total 语义不同）。
- **session join 做**：只取 `directory`/`path` 哈希做 project 归属，不落 `title`；join 不到行 → project 留空不报错（实现清单步骤 4 明确包含）。
- **variant/agent 列**：首版不并入 model 名（保持 model 干净可计价），只在 monitor/diagnostics 里提及。
