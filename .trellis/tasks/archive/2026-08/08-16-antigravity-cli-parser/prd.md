# PRD：antigravity CLI 被动解析器（解除 blocked）

父任务：`.trellis/tasks/08-16-passive-sources-zcode-antigravity-deepseek`（证据见父任务 `research/antigravity-artifacts.md`）。

## Goal

利用本机新出现的真实样本（`~/.gemini/antigravity-cli/conversations/*.db`），把 `antigravity` 来源从 `historical_only`（blocked_no_samples）升级为 parser-backed 被动源：只读解码每个 conversation SQLite 中 `gen_metadata` 表的 protobuf 用量字段，导入 `UsageEvent`，同时保留历史行与既有 `antigravity` stable id。

## Problem / 背景

- ADR-0011 当年因"无 token-bearing fixture"把 antigravity 定为 parserless `historical_only`。
- 现状反转：本机 CLI 侧 `conversations/*.db` 20+ 个 SQLite，含 `gen_metadata` 表；tokscale 参考实现（`ref/repo/tokscale/crates/tokscale-core/src/sessions/antigravity_cli.rs`）已验证该表每行是 `GeneratorMetadata` protobuf，usage 通道在字段 `#4` 子消息。
- IDE 侧 `~/.gemini/antigravity/conversations/*.pb` 无 schema，首版不做（monitor 记 planned）。

## Requirements

### R1 fixture 证据先行（实现第一步，不通过则停下）

- 已完成（2026-08-16，详见父任务 research §2.1/§8）：schema 定稿 `gen_metadata(idx,data,size)`、时间戳来源（`chatModel.#9.#4` + `trajectory_metadata_blob.#2`）、109 行文件字段出现率、`#3==#9+#10` 校验、嵌套路径（usage 在 `#1 chatModel.#4`，顶层 `#4` 恒 36 字节非 usage）。
- 待完成：跨文件字段出现率（81 库抽查）、空/中断会话样本、脱敏合成 blob fixture（`seed_antigravity()`）。
- 产出写回父任务 `research/antigravity-artifacts.md` §8 清单；三项勾完才允许 start。

### R2 来源身份翻转与历史保护（P0，翻转前必须落地）

- `SourceKind::Antigravity` 保留（stable id `antigravity`，ADR-0009）。
- descriptor：`capabilities.parser = true`；`quality`：fixture 验证通道完整后定 `Precise`（若 total 无权威字段且需估算则 `Estimated`，以证据为准）。
- platform monitor：`BlockedNoSamples → Registered`；roots 指向 `~/.gemini/antigravity-cli/conversations`（env `GEMINI_CLI_HOME` 对齐 tokscale 语义）；`next_action` 更新。
- **历史保护三件套（同一 PR、顺序固定，详见 design §5a）**：
  1. schema migration 预置 `token_accounting_version:antigravity = expected`（阻止无界 sync legacy 修复与 serve 自动修复 reset 存量行）；
  2. `--rebuild --source antigravity` 守卫：存在 `source_path_hash` 为空的未归属历史行时拒绝执行并说明原因；
  3. 升级路径测试用真实旧 key 形状（ADR-0009：迁移只改 source 不改 key）。
- `src/commands/sync.rs` 的 parserless 特判分支（520-533、597-624）随翻转调整；`--rebuild` 拒绝逻辑（840-853）改为上述守卫。
- 新增/修订 ADR：记录"样本到位、解除 blocked"决策、证据来源、历史分代语义（解析器时代可重建 / hook 时代只读保护）与 marker 迁移。

### R3 解析与 token 语义

- 每 `.db` 一个 conversation；逐行读 `gen_metadata` blob，手写 protobuf wire 解码（零新依赖）。**解码路径：blob → `#1`（chatModel）→ 内层字段**（顶层 `#4` 不是 usage，跳过）：
  - chatModel.`#4` usage 子消息：`#1` system prompt tokens（常量≈1132）、`#2` non-cached input、`#3` 校验和（= `#9`+`#10`）、`#5` cacheRead（可缺失）、`#9` output（**仅文本**）、`#10` thinking（**独立通道，与 #9 不相交**）、`#11` responseId。
  - chatModel.`#19` responseModel、`#21` display label、`#9.#4` 每代时间戳；会话级 `trajectory_metadata_blob.#2` / `#1.#1`（workspace URI）。
- 归一化（修订）：`input = #2 + #1`、`cache_read = #5`、`output = #9`（仅文本）、`reasoning = #10`（不相交由 `#3==#9+#10` 证明）、**`total = input + cache_read + output + reasoning`**（候选表注明无权威 grand total）。
- model 回填对齐 tokscale `SessionModels`（label→model 映射 + 歧义丢弃 + sole_model + unknown），不用"最近兄弟行"。
- `#3 != #9+#10` → parse issue 计数不改数。

### R3b bounded run 契约

- `--recent-days` 运行：按归一化事件时间过滤，**不推进全历史 cursor、不执行整文件 reset**；随后的全量 sync 必须能恢复窗口外历史（契约 `source-sync-contracts.md:86-88`）。集成测试覆盖。

### R4 增量与幂等

- per-file `FileCursor`（DB 文件 fingerprint：size+mtime+tail signature，grok sidecar 同款）。
- `event_key = antigravity:<hash(convo文件路径)>::<hash(responseId)>`；reparse 时 `reset_path_hashes` 清旧行（pi/kimi 模式）。
- conversation 文件删除：source_file 三态机 missing 保护（历史用量保留）。

### R5 测试（onboarding gate 全项）

- fixture 单测：blob → `UsageEvent` 通道断言（**含 #9/#10 分离与 total 含 reasoning**）；缺 model 回填；占位 model；全零跳过。
- 集成：sync-twice 幂等、append（新行追加）、rewrite/reparse 替换旧行、删除文件保历史、missing root `passive_no_data`、`GEMINI_CLI_HOME` 覆盖、历史行与新导入共存（升级路径测试：真实旧 key 形状 + 无界 sync/serve 不删存量行）、**rebuild 守卫拒绝/放行**、**bounded run 不 reset 不推 cursor 且窗口外可被全量恢复**。

### R6 文档

- `docs/agents/passive-source-candidates.md` Antigravity 行更新为 Approved（注明 CLI 工件族与 IDE `.pb` 仍 planned）。
- `README.md` / `README.zh-CN.md` / docs；ADR 修订链接。

## Acceptance Criteria

- [ ] `llmusage source-status`（无 `--source` 参数，核对输出中 antigravity 行）报 `passive_ready`（有数据时）且不再 `historical_only`。
- [ ] **P0**：预置 marker 的 migration 合入后，无界 sync 与 serve 均不把存量 antigravity 行判为 legacy、不删除；`--rebuild --source antigravity` 在存在未归属历史行时拒绝。
- [ ] 本机真实数据 sync 后 token 总量与手工解码 2-3 个 conversation 的结果一致（抽查核对记录在 research；**含 #9+#10 都计入**）。
- [ ] R5 全部测试通过（含 bounded run 与 rebuild 守卫）；`cargo test --all-features -- --test-threads=1`、`just ci` 全绿。
- [ ] 存量 antigravity 历史行不被破坏/重复计数。
- [ ] 无轨迹/提示词文本落库；fixture 脱敏可入仓。
- [ ] ADR 与候选表更新完成。

## 非目标

- IDE 侧 `conversations/*.pb` 解析（无 schema，monitor `planned`）。
- 不装 tokscale 式 RPC 钩子（ADR-0011 被动原则）。
- 不做 battle_mode/executor 元数据的语义展开。
