# Antigravity 本地产物证据（解除 blocked 的依据）

调研日期：2026-08-16，样本来自本机 `C:\Users\lyh\.gemini\`。llmusage 现状：`SourceKind::Antigravity` 存在但 `capabilities.parser = false`（`historical_only`），platform monitor `blocked_no_samples`（ADR-0011）。本机现在有真实样本，阻塞前提已消失。

> **2026-08-16 二次校验修订（关键）**：初稿把 usage 写成 `gen_metadata` 顶层 `#4`——**错误**。正确路径是 `blob.#1（chatModel）→ #4（usage）`，与 tokscale `antigravity_cli.rs:224-225`（`message_field(blob, 1)` → `message_field(chat_model, 4)`）一致；顶层 `#4` 也存在（109/109 行、恒 36 字节）但**不是 usage**，按初稿实现会稳定读到错误字段且不会因缺字段而失败。本机最大 conversation（109 行）逐行 wire 解码见 §2.1。

## 1. 本机目录布局（实测）

```
~/.gemini/
├── antigravity-cli/                  ← Antigravity CLI（独立产品）
│   ├── conversations/<uuid>.db       ← ★ 每 conversation 一个 SQLite（81 个，262KB~1.9MB）
│   ├── conversation_summaries.db
│   ├── brain/<uuid>/…                ← scratch/ 元数据
│   ├── history.jsonl（29KB）
│   ├── bin/ builtin/ cache/ crashes/ implicit/
├── antigravity/                      ← Antigravity IDE（VS Code fork）
│   ├── conversations/<uuid>.pb       ← ★ 原始 protobuf 会话（1.2MB 级）
│   ├── agyhub_summaries_proto.pb / antigravity_state.pbtxt
│   ├── annotations/ brain/ code_tracker/ context_state/ knowledge/ …
├── antigravity-ide/ antigravity-backup/ antigravity-browser-profile/
```

## 2. CLI 侧：`conversations/*.db` 的 `gen_metadata` 表（推荐解析目标）

实测表结构（已定稿，81 个库一致）：

- `gen_metadata(idx INTEGER PK, data BLOB, size INTEGER)`
- `trajectory_metadata_blob(id TEXT, data BLOB)`
- 其余表：`battle_mode_infos, executor_metadata, parent_references, steps, trajectory_meta`

最大 conversation `609c13f0-….db`：1,912,832 bytes、109 行 gen_metadata。

### 2.1 wire 结构（109/109 行实测 + tokscale `antigravity_cli.rs:15-40` 文档注释互证）

**解码路径：`data blob` → `#1`（chatModel 消息）→ 内层字段。**

| 位置 | 出现率（109 行文件） | 含义 |
| --- | --- | --- |
| 顶层 `#1`（len-delim） | 109/109 | **chatModel 消息**（解码入口） |
| 顶层 `#4`（len-delim） | 109/109 | **不是 usage**，恒 36 字节，含义未知——必须跳过 |
| chatModel.`#4` | 109/109 | **usage 子消息**（真正的用量在这） |
| chatModel.`#19`（string） | 109/109（本文件） | responseModel（机器 id，如 `gemini-3-flash-a`）；tokscale 观测续写 turn 可缺失 |
| chatModel.`#21`（string） | 109/109（本文件） | model 显示标签（如 `Gemini 3.6 Flash (High)`），作 SessionModels join key |
| chatModel.`#9.#4` | 109/109 | 每代 wall-clock 时间 `{#1: 秒, #2: 纳秒}`（绝对时间戳） |
| usage.`#1`（varint，常量≈1132） | 109/109 | fixed system-prompt tokens |
| usage.`#2` | 109/109 | non-cached input |
| usage.`#3` | 109/109 | **= #9 + #10**（output+thinking 校验和，109/109 成立；agent-walker 同款不变量） |
| usage.`#5` | 106/109（首行缺失） | cacheRead（无缓存前缀的行不出现） |
| usage.`#6`（varint，恒 24） | 有 | 映射未知 |
| usage.`#8`（string） | 本文件 0 行（审阅样本有） | 映射未知，出现率随文件而异 |
| usage.`#9` | 109/109 | output（**仅文本**，thinking 在 #10，二者不相交） |
| usage.`#10` | 109/109 | thinking / reasoning |
| usage.`#11`（string） | 109/109 | responseId（dedup 锚） |
| `trajectory_metadata_blob.#2` | 实测 `{#1: 1785140245 秒, #2: 657105100 纳斯}` | 会话 created-at Timestamp |
| `trajectory_metadata_blob.#1.#1`（string） | 实测存在 | workspace URI |

tokscale 参考实现（`ref/repo/tokscale/crates/tokscale-core/src/sessions/antigravity_cli.rs`）：

- `SELECT data FROM gen_metadata ORDER BY idx` → `blob.#1`（chatModel）→ `#4` usage、`#19` model、`#9.#4` 每代时间；会话时间/workspace 在 `trajectory_metadata_blob.#2` / `#1.#1`。
- 手写 wire-format 读取器，**不需要 .proto 文件**；未知字段按 wire type 跳过。
- `SessionModels`（103-212 行）：`#21` 标签 → `#19` 机器 id 的映射表；一个标签配到两个**计价不同**的 id 时丢弃该映射（先用 `pricing::aliases::resolve_alias` 归一再比较）；全文件只有一个被标签确认的 id 时 `sole_model` 兜底；两者皆无 → `unknown`。**不是**"最近兄弟行回填"——续写 turn 缺 `#19` 时按标签/sole_model 恢复，歧义即 unknown。
- 模型占位符（IDE 侧 antigravity.rs）：`MODEL_PLACEHOLDER_M26` → `claude-opus-4-6`、`model_placeholder_m84` → `gemini-3-flash-preview`——CLI 侧未见，遇到再映射。

### 2.2 已知缺口

- gen_metadata 无 cache write/creation 字段；官方 statusline `current_usage` 含 `cache_creation_input_tokens`（审阅提供的官方文档参考，未本地复核）→ 首版按 tokscale 通道（无 cache_creation），缺口记入候选表。

## 3. IDE 侧：`conversations/*.pb`

原始 protobuf 会话流（未 schema 化）。tokscale 不直接读 IDE 的 `.pb`（它靠自家 RPC 缓存 `~/.config/tokscale/antigravity-cache/`，那是钩子产物，llmusage 明确不装钩子，**不可复制**）。IDE `.pb` 解码成本高（需要逆向 schema），首版不做，monitor 记 `planned`。

## 4. 与 llmusage 现有 antigravity 身份的整合

- 复用 `SourceKind::Antigravity`（stable id `antigravity`，ADR-0009 已定）；把 descriptor `capabilities.parser` 翻 true、quality 从 `TotalOnly` 升 `Precise`（通道可精确到请求级；total 无权威 grand total，为通道求和——候选表须注明）。
- **P0 历史保护（2026-08-16 三次校验新增，翻转前必须解决）**：
  - 危险链条（已逐环验证）：`reset_for_source` 删除 `source='antigravity'` 全部事件（`schema.rs:225`）；`has_legacy_token_accounting`（`schema.rs:186`）= marker 缺失且有行 → legacy；无界 sync 的 legacy 修复与 serve 自动修复会 reset legacy 子集；hook 时代事件**无 `source_file` 路径归属**，`lossy_rebuild_risk`（`source_file.rs:198` 起）只看 source_file 缺失路径 → guard 失明。后果：翻转后首次无界 sync/serve 可能删掉 ADR-0009 迁移的、conversations/*.db 里不存在而无法重建的旧 Gemini 事件。
  - 对策（写进子任务 design/implement 与新 ADR）：① **marker 迁移先行**——新 schema migration 在 parser 注册同 PR 内预置 `token_accounting_version(antigravity) = expected`，让两条自动修复路径从第一天起就不再把存量行判为 legacy；② **rebuild 守卫**——`--rebuild --source antigravity` 检测 `source_path_hash` 为空/NULL 的未归属事件数 > 0 时拒绝执行（提示历史不可重建与手动导出建议），解析器时代的新事件有文件归属、不受限；③ 升级路径测试用真实旧 key 形状（ADR-0009: 迁移只改 source 不改 event key）。
- 历史行：`historical_only` 时期无新增行，存量行不受影响；新解析器从 `conversations/*.db` 全量回填。ADR-0011 的”parserless”决策需要新 ADR 或修订记录推翻（写明证据来源 + 上面的历史保护方案）。
- **存量 event_key**：gemini→antigravity 迁移”event keys remain unchanged”——旧 key 保留原始形状。
- platform monitor `parser_status`: `BlockedNoSamples` → `Registered`，roots 指向 `~/.gemini/antigravity-cli/conversations`。**env 钉死 `GEMINI_CLI_HOME`，语义 = gemini 根（默认 `~/.gemini`），相对路径 `antigravity-cli/conversations`**——与仓库 gemini monitor（`platform_monitor.rs:304` 已用 `GEMINI_CLI_HOME`，`home_relative: “.gemini/tmp”`）和 tokscale 保持同一语义，不再自造 `ANTIGRAVITY_CLI_HOME`。
- sync 命令里 parserless 特殊分支（`src/commands/sync.rs:520-533, 597-624`）与 `--rebuild` 拒绝逻辑（840-853）需要随 descriptor 翻转自然失效/调整。

## 5. token 语义与质量 label（2026-08-16 三次校验修订：#9/#10 不相交）

- **tokscale 注释明确**（antigravity_cli.rs:29-30）：`#9` = output (**text**) tokens、`#10` = thinking / reasoning tokens，二者**不相交**；`#3 = #9 + #10` = 总 output（本机 109/109 成立，本机例 `#9=234, #10=50, #3=284`）。
- 归一化（修订）：`input = #2 + #1`（non-cached input + fixed system prompt，对账后确认口径）、`cache_read = #5`、`output = #9`（**仅文本 output**）、`reasoning = #10`（**独立通道，与 output 不相交——由 #3 不变量证明**）。
- **total = input + cache_read + output + reasoning**：llmusage 契约规定 reasoning 默认诊断不计 total，**但"source contract proves it is disjoint from output"时例外**——`#3 == #9+#10` 就是这个证明（tokscale 140 turns + 本机 109 行双证）。初稿"output=#9 含 thinking"会静默少算 #10，作废。
- `#3` 同时作行级校验：`#3 != #9 + #10` → parse issue 计数不改数。
- 每行一个 responseId → 每请求粒度 → `precise`；total 无权威 grand total 字段，为通道求和（候选表注明；`#3` 只覆盖 output+thinking 侧，input/cache 侧无校验）。

## 6. 增量游标

- 每个 conversation 一个独立 SQLite 文件 → 复用 per-file `FileCursor`（fingerprint = size+mtime+tail signature，对 DB 文件同样适用；grok 的 sidecar fingerprint 同款思路）。
- 行级去重：`event_key = antigravity:<convo哈希>::<hash(responseId)>`；reparse 时 `reset_path_hashes` 清旧行（pi/kimi 模式）。
- WAL：CLI 库未见 `-wal` 残留（快照式写入），只读打开即可。

## 7. 隐私边界

- 读取：仅 `gen_metadata`（+必要的会话元数据表列）。`steps`/`trajectory_*` 可能含完整轨迹文本，不读（时间戳/workspace 取 `trajectory_metadata_blob` 的 `#2`/`#1.#1` 两个标量字段）。
- 持久化：归一化 usage、model、时间戳、responseId 哈希。
- fixture：从本机 `.db` 提取 `gen_metadata` blob 后**重编码为只含 usage/model/时间戳的合成行**（真实 blob 可能内嵌 prompt 片段，不能直接入仓）。

## 8. 待 fixture 验证清单（实现第一步）

- [x] `pragma table_info(gen_metadata)` 实际列名与 blob 列定位（2026-08-16：`idx/data/size`，见 §2）。
- [x] 时间戳来源（2026-08-16：行内 `chatModel.#9.#4` 每代时间 + `trajectory_metadata_blob.#2` 会话 created-at）。
- [x] `#4` 子消息各字段在本机样本的实际出现率（2026-08-16：§2.1 表，109 行文件）。
- [x] model id 分布与占位符出现率（2026-08-16：本文件 109/109 有 `#19`；占位符未见——tokscale 观测续写 turn 可缺）。
- [x] 跨多个文件的 `#4(顶层)/#5/#19/#21/#8` 出现率（2026-08-16 晚，`antigravity_crossfile_sample.py` 扫全部 81 库 1528 行）：
  - 顶层 `#4`：1528/1528（100%，恒非 usage，跳过策略成立）；usage 子消息：1528/1528（100%）。
  - `#3 == #9 + #10`：1434 行三字段齐全全部成立，**0 违反**；其余 94 行为字段缺失（未校验），无错报。
  - `#5` cache_read：1419/1528（92.9%）；`#19` model：1521/1528（99.5%，7 行缺失→SessionModels 回填必要）；`#21` label：1508/1528（98.7%）；`#8`：1522/1528（**常见**，非深挖文件观测的 0）；`#11` responseId：1522/1528。
  - 全零 usage 行：6（跳过语义必要）；distinct models：`claude-opus-4-6-thinking, gemini-3-flash-a, gemini-3.6-flash, gemini-3.6-flash-tiered, gemini-3.7-flash`；labels 含 `Gemini 3.6 Flash (High)/(Low)` 同模型双档位（SessionModels 歧义丢弃逻辑必须实现）。
- [x] 空 conversation / 中断会话样本（2026-08-16 晚）：`09615478-…db` 与 `af46f3e2-…db` 均为 **0 行 gen_metadata** 的真实空会话（解析器必须干净跳过，不报错）；`10784ed2-…db` 为 1 行全零样本。中断容错（截断 blob / 畸形 wire）以合成 fixture 单测覆盖：解码器逐字段消费、遇截断/未知 wire type 丢弃该行并计 parse issue，不 panic。
- [x] 脱敏合成 blob fixture 入仓（`seed_antigravity()` 写合成字节）：解析器测试用手工编码 wire 字节构造（usage/model/label/时间戳齐全 + 顶层 `#4` 干扰 + 未知字段 + 全零行 + 截断 blob），全部合成脱敏，可入仓。

## 9. 真实数据抽查对账（2026-08-16 实现 日，验收项）

- 首次 `sync --source antigravity`（本机 83 个 conversation DB，1579 行 gen_metadata）：导入 **1573 事件**（= 1579 行 − 6 全零行，与独立 Python 解码器逐行结果完全一致）。
- 六通道总量对账（Python 手工解码 vs llmusage `usage_event` 求和）：`input=11,550,544`、`cache_read=55,989,479`、`output=245,710`、`reasoning=248,087`、`total=68,033,820`、`events=1573` —— **全等**（#9 文本 output 与 #10 thinking 都计入，total = input+cache+output+reasoning）。
- 二次 sync 零新增（83 skipped）；`source-status` 报 `antigravity: passive_ready / precise / accounting=current`。
- 库间 6 个全零行被跳过、2 个空会话（0 行 gen_metadata）干净跳过——与 §8 预测一致。
