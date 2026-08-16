# Design：antigravity CLI 被动解析器

依据：父任务 `research/antigravity-artifacts.md`；tokscale 参考 `ref/repo/tokscale/crates/tokscale-core/src/sessions/antigravity_cli.rs`；llmusage 现有 pi/grok 文件型解析器模式。

> **P0 前置（翻转 parser 前必须落地，否则会删历史）**：见 §5a 历史保护设计。

## 1. 组件与边界

```
src/parsers/antigravity.rs        新增（grok.rs 为结构范本）
src/parsers/source_files.rs       list_antigravity_conversation_files()
src/integrations/antigravity.rs   现有 hook 清理逻辑保留；新增 conversations 根解析（env GEMINI_CLI_HOME → ~/.gemini）
src/domain/source_descriptor.rs   capabilities.parser=true；quality 待 fixture 定（Precise 预期）
src/domain/platform_monitor.rs    parser_status=Registered；roots 加 conversations 目录；IDE .pb 单列 planned
src/store/migrations.rs           m_0XX：预置 antigravity token-accounting marker（§5a）
src/commands/sync.rs              rebuild 守卫（§5a）；收缩 parserless 特判（520-533、597-624、840-853）
src/parsers/mod.rs                导出（protobuf+SQLite 源，不进 JSONL bounded_contract_parse 测试，
                                  但 recent-days/bounded-run 契约同样适用——见 §6b）
docs/adr/ADR-00xx-antigravity-cli-parser.md   新决策记录（含历史保护）
```

零新依赖：protobuf 用手写 varint/wire 解码器（~150 行，tokscale 同款做法），rusqlite 已有。

## 2. 发现层

- 根：`$GEMINI_CLI_HOME/antigravity-cli/conversations`，`GEMINI_CLI_HOME` 默认 `~/.gemini`。**沿用仓库 gemini monitor 已有的 `GEMINI_CLI_HOME`（`platform_monitor.rs:304`）与 tokscale 的同一语义（env = gemini 根，不是 conversations 目录）**，不自造 `ANTIGRAVITY_CLI_HOME`。
- 收集 `*.db`（跳过 `-wal/-shm`）；`SourceFileListing { paths, errors }` 对齐 kimi/pi。

## 3. protobuf wire 解码器（核心新件）

```rust
struct GenMetadataUsage {
    system_prompt: u64,   // #1，常量≈1132
    input: u64,           // #2 non-cached input
    checksum: u64,        // #3 == #9 + #10（output+thinking 校验和）
    cache_read: u64,      // #5，无缓存前缀的行缺失
    output: u64,          // #9（仅文本 output；thinking 在 #10，不相交）
    thinking: u64,        // #10
    response_id: String,  // #11（dedup 锚）
}
/// 解码路径：blob → #1（chatModel）→ { #4 usage, #19 model, #21 label, #9.#4 时间戳 }
fn decode_gen_metadata(blob: &[u8]) -> Option<AntigravityGenMetadata>
```

- 标准规则：field key = `(field_no << 3) | wire_type`；varint / len-delimited 两种 wire type 足够（usage 全部 varint，responseId/model/label 是 string）。
- **嵌套契约（勿重蹈初稿覆辙）**：usage 在 `chatModel(#1).#4`；顶层 `#4`（恒 36 字节）不是 usage，解码器只从 `#1` 内层取数，遇到顶层 `#4` 直接跳过。
- 未知字段（含 usage 内 `#6` 恒 24、偶见 `#8` string）按 wire type 跳过；畸形 blob → parse issue 计数 + 丢弃行（不得 panic）。
- 完整性校验：`#3 != #9 + #10` → parse issue 计数（109/109 实测成立），不改数。
- 每代时间戳：`chatModel.#9.#4 = {#1 秒, #2 纳斯}`；会话级 created-at：`trajectory_metadata_blob.#2` 同构；workspace URI：`trajectory_metadata_blob.#1.#1`（哈希化持久化）。
- 单测直接用手写字节序列构造（含顶层 `#4` 干扰字段、未知字段、截断 blob）。

**model 回填规则（对齐 tokscale `SessionModels`，antigravity_cli.rs:103-212，替代初稿的"最近兄弟行"）**：

1. 先扫全文件建 `#21 label → #19 model` 映射；同一 label 配到两个计价不同的 `#19`（先 `pricing::aliases` 归一比较）→ 丢弃该 label 映射。
2. 全文件恰好一个被 label 确认过的 `#19` → `sole_model` 兜底。
3. 行自身有 `#19` 用之；无 → label 映射；再无 → sole_model；都不行 → `unknown`（宁可 unknown 也不猜）。

## 4. 每 conversation 处理流程

1. 只读打开 `conversations/<uuid>.db`；`SELECT <blob列>, <时间列> FROM gen_metadata`（列名以 fixture 步骤实测为准，design 预留 blob 列/时间列两个常量）。
2. 逐行解码 → `GenMetadataUsage`；全零行跳过；`responseId` 空 → 用行 rowid 兜底并计数。
3. model 回填：先缓存本文件已见 model；缺失行用最近兄弟行；全文件缺失 → fallback `antigravity-unknown`。
4. `UsageEvent`：
   - `event_key = antigravity:<path_hash>::<hash(response_id)>`
   - `event_at` = `chatModel.#9.#4` 每代时间（fallback `trajectory_metadata_blob.#2` 会话 created-at）
   - tokens（**修订：#9/#10 不相交**）：`input = #2 + #1`（system prompt 并入，fixture 对账后确认）、`cache_read = #5`、`output = #9`（**仅文本 output**）、`reasoning = #10`（独立通道）、**`total = input + cache_read + output + reasoning`**（reasoning 与 output 的不相交由 `#3 == #9+#10` 不变量证明，满足 llmusage 契约"proves disjoint"例外，计入 total；初稿"output 含 thinking"会静默少算 #10，作废）
   - 行级校验：`#3 != #9 + #10` → parse issue 计数不改数
   - cost：catalog 无 antigravity 条目 → `Unpriced`
5. 文件级聚合进 `SyncShard`；`FileCursor` 记 fingerprint+offset（offset 对 SQLite 用页尾/行数水位，参照 grok 的 sidecar 指纹而非字节 offset——DB 文件字节 offset 不稳定，fingerprint + 全文件重解析 + event_key 幂等更稳）。

**游标决策**：DB 文件不做字节续读；`decide_file_replay` fingerprint 不变 → 整文件跳过（skipped_files++）；变化 → 全文件重解析 + `reset_path_hashes` 替换旧行 + event_key 幂等兜底。代价是 append 场景全量重解析（conversation 文件 ≤1.1MB，可接受），换来无需理解 SQLite 页结构。

## 5. sync 命令集成与历史保护（P0）

- antigravity 从 parserless 白名单移除后自然走 `registry::registered_parsers()` 通用路径；`historical_only` 状态推导保留（供未来其他 parserless 源），但 antigravity 不再命中。

### 5a. 历史分代与不可重建数据保护（翻转 parser 的前置条件）

**问题链条（已逐环代码验证）**：hook/gemini 时代的存量事件没有 `source_file` 路径归属 → ① 无界 sync 的 legacy token-accounting 修复（`has_legacy_token_accounting`，`schema.rs:186`：marker 缺失 + 有行即 legacy）与 serve 自动修复会 reset 该源；② `reset_for_source`（`schema.rs:225`）删除 `source='antigravity'` 全部事件；③ `lossy_rebuild_risk`（`source_file.rs:198` 起）只检查 source_file 里缺失的路径，对无归属行失明。ADR-0009 迁移只改 source 不改 event key，这些行在 `conversations/*.db` 里不存在、**删了就永久丢失**。

**对策（三件套，同一 PR 内落地，顺序固定）**：

1. **marker 迁移先行**：新 schema migration `m_0XX_preset_antigravity_token_accounting` 在 parser 注册之前预置 `meta[token_accounting_version:antigravity] = expected_token_accounting_version(Antigravity)`。效果：两条自动修复路径（sync legacy 修复、serve 修复）从翻转当天起就不再把存量行判为 legacy，不会 reset。不依赖"首次 sync 成功后打 marker"——那条路径会在 parser 首跑前就被修复逻辑抢先清库。
2. **rebuild 守卫**：`--rebuild --source antigravity` 执行前检查未归属历史行数 `SELECT COUNT(*) FROM usage_event WHERE source='antigravity' AND (source_path_hash IS NULL OR source_path_hash='')`；> 0 时**拒绝执行**，报错说明存在不可从本地工件重建的 hook 时代历史、建议先导出备份。解析器时代新事件都有文件归属，不受此守卫影响（此后 rebuild 语义与 kimi/grok 一致）。
3. **升级路径测试**：用**真实旧 key 形状**（从 ADR-0009 迁移测试/存量行采样，不假设前缀）预置存量行，断言：翻转后首次无界 sync 不删除存量行；serve 不触发该源修复；`--rebuild` 被守卫拒绝；存量行与新导入行在报表中共存不重复。

新 ADR 记录：证据来源、marker 迁移、rebuild 守卫、"解析器时代事件可重建 / hook 时代事件只读保护"的分代语义。

### 5b. bounded run（--recent-days）契约

- `recent_cutoff` 有值时：按归一化事件时间过滤（`#9.#4` 时间 < cutoff 的行不导）；**不推进全历史 cursor、不执行整文件 reset**——fingerprint 变化的文件在 bounded 模式下只解析新增内容且按窗口过滤，重置/重放留给随后的全量 sync（契约 `source-sync-contracts.md:86-88`，行为对齐 pi/kimi 的 bounded 处理）。
- 解析器时代的 cursor 语义不受 bounded run 影响：全量 sync 仍能恢复窗口外的历史。

## 6. 测试设计

- 单测（`src/parsers/antigravity.rs`）：wire 解码器字节级用例（含**顶层 `#4` 干扰字段**、未知字段、截断 blob、`#5` 缺失行）；合成 DB（rusqlite 建表插合成 blob）→ UsageEvent 断言；`#3 != #9+#10` 校验告警；SessionModels 回填（label 映射、歧义丢弃、sole_model、unknown）；全零跳过。
- 集成（`tests/sync_regression.rs`）：
  - `antigravity_sync_twice_is_idempotent`
  - `antigravity_append_replays_file_and_replaces_stale_rows`（fingerprint 变化 + event_key 幂等）
  - `antigravity_deleted_conversation_preserves_history`
  - `antigravity_missing_root_reports_no_data`
  - `antigravity_cli_home_override`（`GEMINI_CLI_HOME` 指向临时 gemini 根）
  - `antigravity_upgrade_from_historical_only_keeps_legacy_rows`（先插**真实旧 key 形状**的存量行再 sync，断言不重复、不破坏、无界 sync/serve 不触发 legacy 修复删除）
  - `antigravity_rebuild_refused_while_unattributed_history_exists`（P0 守卫：有未归属存量行时 `--rebuild` 拒绝；清理后可重建）
  - `antigravity_recent_days_run_skips_reset_and_window_filters`（bounded：不推进 cursor、不 reset，窗口外行留给全量 sync 恢复）
- 状态测试：`source-status` 输出 `passive_ready`/`passive_no_data`、quality 标签；`historical_only` 用例改由其他 fixture 驱动或删除。

## 7. 风险与开放问题

| 风险 | 缓解 |
| --- | --- |
| schema 已定稿（`gen_metadata(idx,data,size)`）；剩余变数是跨文件字段出现率与空/中断样本 | R1 fixture 步骤先行（research §8 剩余三项），不勾完不写解析器（stop rule） |
| usage 字段出现率低（如只有 output） | 通道缺失按 0 处理 + issue 计数；quality 降 `Estimated` 并写入候选表 |
| blob 内嵌 prompt 文本 | fixture 一律重编码合成；解析器只读数值字段，string 只取 responseId/model/label |
| 旧钩子时代 event_key 冲突 | 用真实旧 key 形状做升级路径测试（ADR-0009：迁移不改写 key） |
| protobuf schema 变更（CLI 升级） | wire 解码对未知字段天然容忍；出现率骤降时 parse issue 可观测 |
| cache_creation 通道缺失（官方 statusline 有、gen_metadata 未见） | 首版无该通道，候选表记缺口；出现对应字段再补 |
