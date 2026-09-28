# Research: Sync 诊断与性能的上游实现比较

- Query: 比较已更新的 ccusage 与 tokscale，针对本次 Codex skipped、Grok usage_incomplete、Antigravity 缺失数据库与 legacy、增量扫描及写入耗时，识别可复用机制和不适用的行为。
- Scope: internal；读取两个参考仓库及 llmusage 的约束文档，不执行参考 CLI、sync、rebuild 或用户数据扫描。
- Date: 2026-09-26
- Active task: `.trellis/tasks/09-26-sync-diagnostics-performance`；本研究不改变 planning 状态。

## Findings

### 1. 版本、证据范围与规范

主会话已执行参考仓库更新，并给出下列提交锚点。研究者未执行任何 Git 操作；以下路径及行号来自更新后的本地文件。若参考仓库再次更新，行号必须按对应提交重新核对。

| 简称 | 本地根目录 | 提交锚点 | 适用指令 |
| --- | --- | --- | --- |
| C | `ref/repo/ccusage/` | `732c7a6362f3d86a4992d2ad7071b6532161a396` | 根 `AGENTS.md`、`rust/adapters/AGENTS.md`；已读取 `.agents/skills/agent-sources/SKILL.md` |
| T | `ref/repo/tokscale/` | `1d9a9395418efc6952944b794097935d7d6fa1e8` | 根 `AGENTS.md`；在 `crates/` 下未发现更近的 `AGENTS.md` |

本报告的 `C:` 与 `T:` 是上述本地根目录的前缀。例如 `C:rust/adapters/codex/src/parser.rs:224-239` 指向 `ref/repo/ccusage/rust/adapters/codex/src/parser.rs`。参考项目源码是本研究的外部一手来源；没有借助 README 的性能宣传推导速度结论。

已读取本项目 `.trellis/workflow.md`、任务 `prd.md`、`docs/agents/domain.md`，以及下列相关规范和 ADR：

- `.trellis/spec/llmusage/backend/source-sync-contracts.md:46-57`：Claude 按项目重放，Codex 按文件 offset 续读；`SKIPPED` 文件计数与 record issue 不同。
- `.trellis/spec/llmusage/backend/source-sync-contracts.md:123-148`：Antigravity 按产品分组、DB/WAL 快照、完整 staging、失败保留历史、显式有损门禁。
- `.trellis/spec/llmusage/backend/source-sync-contracts.md:484-531`：4 MiB record 上限、newline durable cursor、取消、隐私、每源最多 8 个样本、闭集 reason。
- `.trellis/spec/llmusage/backend/token-accounting-contracts.md:45-74,97-159`：Codex/Grok/Antigravity 通道、回放、来源隔离和 legacy 保留。
- `docs/adr/0002-sync-shard-as-commit-protocol.md`：reset、events、facts、cursors 由 writer 在原子 shard 协议内提交。
- `docs/adr/0006-source-file-state-machine.md`：missing 与 deleted_by_user 分离；2026-07-26 补充 newline durable boundary。
- `docs/adr/0007-llmusage-error-surface.md`：公共错误须有可区分的调用方语义。
- `docs/adr/0017-antigravity-native-accounting.md`：安装版本 descriptor 是字段语义依据；重试 aggregate 不能与 attempts 相加。

### 2. 结论与采纳矩阵

| 本次问题 | 可用参考机制 | 建议 | 必须保留的边界 |
| --- | --- | --- | --- |
| Codex `skipped=13`，只展示 8 条 | C 在 JSON 解析前筛选 record 类型；T 对续读 state 与 newline 做一致性校验 | 采纳筛选与验证思想；改进跳过原因说明 | 本项目已有 4 MiB/8 KiB 受限 reader，不能退回无上限 `read_line/read_until`；13 条的实际内容未在本研究读取 |
| Grok `accounting=13 usage_incomplete` | C 区分 top-level usage 与 `modelUsage`；T 提供逐字段别名读取 | 用于设计缺失字段诊断和脱敏 fixture；是否补充解析须由原始结构证据决定 | 缺失字段默认 0 不能等价为数据完整；不混入 context occupancy，不改变 unpriced 和一 usage 一 event 契约 |
| Antigravity legacy 与 IDE 缺失 DB | C 使用只读事务与 `Result<Option<Vec<_>>>` 区分错误/空库/成功；T 跟踪 DB 的 WAL | 借鉴明确状态、完整快照与 DB/WAL 观察；本项目许多机制已存在 | 旧历史不是可丢弃 cache；不自动修复或开启 `--allow-lossy-rebuild` |
| 2.5 GB 扫描与大文件 | C 按文件大小分配 worker；T 复用 fingerprint 和 Codex parse state | 先做 I/O 与阶段计量；只优化实测重复工作 | T 的 Codex append 仍哈希完整旧前缀；不能据此断言为纯尾部 I/O |
| `WRITE=105.2s` | T 按 dirty key 一次分组，仅更新受影响 shard；成功后才清 dirty | 借鉴限定受影响集合及避免重复遍历 | T 写二进制 cache，C 生成报告；两者不承担本项目 SQLite event/bucket/fact/cursor 的相同工作量 |

### 3. Codex：筛选有参考价值，跳过诊断不能被删除

#### 源码事实

1. C 用 128 KiB `BufReader`，逐行复用 `Vec`，先调用 `codex_line_usage_kind` 再做 typed JSON 解析；无关类型直接跳过，serde 错误也直接继续。见 `C:rust/adapters/codex/src/parser.rs:162-170,224-248`。`codex_line_usage_kind` 保留 `turn_context`、`token_count`、`thread_settings_applied`，另有 headless usage 入口；对不同空白写法有补充扫描。见同文件 `475-548`。
2. C 的 shared JSONL helper 使用 byte-line 与可选 prefilter，再以 `serde_json::from_slice(...).ok()` 丢弃失败行。见 `C:rust/adapters/common/src/jsonl.rs:47-61`。静默继续属于参考项目的报告读取策略，不能据此证明本次 13 条 harmless 或删除 llmusage 的 issue 计数。
3. C 的 `reader.read_until` 和 T 的 `reader.read_line` 都没有在所读 loop 内施加本项目的 4 MiB record 上限。见 `C:rust/adapters/codex/src/parser.rs:224-239`、`T:crates/tokscale-core/src/sessions/codex.rs:478-507`。从这些 loop 不能推导受限内存处理超长 record 的保证。
4. 本项目明确规定 oversized Codex 的前 8 KiB 若能分类为其他类型，则记 skipped；完整 `token_count` prefix 可恢复，无法解析的 token_count 保持 oversized；未知 junk 保持 oversized。每源样本最多 8 条，计数继续增长。见 `source-sync-contracts.md:498-512`。该规则能解释“13 条计数只列 8 条样本”的呈现，但不能确认用户这 13 条的具体 record 类型。

#### 子任务设计输入

- 先让诊断区分正常非用量大 record、未知超长 record、坏 JSON、usage accounting；展示记录计数与样本截断提示，例如保留“展示 8 / 共 13 条”的语义。最终文案由主任务统一。
- 若新增 prefilter，必须保留累计基线、模型、service tier、fork/replay、turn/tool facts 所需的状态记录。仅匹配 `token_count` 会误删状态依赖。
- 使用构造 fixture 验证紧凑 JSON、空白变体、escaped content 内伪 marker、有效 token_count、超过上限的 non-usage record、截断 EOF、无 newline 的完整 EOF、取消。
- 不将改进诊断升级为 accounting 规则变更。C 已使用父 usage prefix + rewritten burst fallback（同文件 `173-218`），而本项目契约仍规定首 16 KiB marker 加首两条同秒（`token-accounting-contracts.md:59-67`）；直接换算法需要独立差异分析与 fixtures。

### 4. Grok：区分结构缺失、字段缺失与已有 fallback

#### 源码事实

- C 仅读取 `turn_completed` 且包含 `params.update.usage` 的 record。`GrokUsage` 同时支持 top-level 数值和 `modelUsage`，存在非空 map 时按模型读取，缺 map 时回到 top-level。见 `C:rust/adapters/grok/src/parser.rs:67-102,207-241,334-363`。该结构可用于构造真实字段覆盖矩阵。
- C 的缺失数值、null、字符串、负数与非整数会被 `lenient_u64` 归为 0。见 `C:rust/adapters/common/src/jsonl.rs:64-83`。采用这种默认值能减少报错，但不会证明字段已提供。llmusage 不能靠默认 0 消除 `usage_incomplete`。
- C 用各子通道计算报告量并忽略 `model_usage.total_tokens`，还从 `cost_usd_ticks` 获取费用。见 `C:rust/adapters/grok/src/parser.rs:281-285,321-327`。本项目 `totalTokens` 权威、Grok unpriced 且禁止转换 costUsdTicks，见 `token-accounting-contracts.md:97-106`；不采纳这两项参考行为。
- T 的 `usage_value` 同样对缺失字段默认 0。其 output 减 reasoning，供 additive `TokenBreakdown` 使用；本项目保留 raw output 且 reasoning 仅诊断。见 `T:crates/tokscale-core/src/sessions/grok.rs:174-259`。通道内部表示不同，不能逐字段复制。
- T 在有 usage 时仍追加晚于最新 usage timestamp 的 fallback activity。见 `T:crates/tokscale-core/src/sessions/grok.rs:499-518`。本项目规定一个 session 只要有 usage，就不能加入 `_meta.totalTokens`/signals reconciliation，见 `token-accounting-contracts.md:103-106`。不采纳该混合策略。
- 两个项目的 identity 假设存在差异：C 在跨文件层按 `eventId|model` 去重，见 `C:rust/adapters/grok/src/loader.rs:45-61`；T 明确记录 eventId 可复用，使用 `usage_index:event_id` 保证文件内唯一，见 `T:crates/tokscale-core/src/sessions/grok.rs:408-430`。因此，不能用任一参考 identity 直接替换 llmusage 的持久 identity。

#### 子任务设计输入

- 当前日志只给 `usage_incomplete`。在最小脱敏证据里记录字段存在性、类型、是否有 modelUsage，以及该事件是否仍存储；不记录原始 JSON、prompt、assistant text、error_message 或完整路径。
- reason 采用闭集或有限字段位图，以区分 missing-input、missing-output、missing-total、unknown-shape 等经过核实的情况；并使诊断能够定位文件/offset。是否缺 total 应按本项目已有合法 fallback 单独定义，不预设为 error。
- 若只有 `modelUsage` 完整而 top-level 缺失，先验证同一 usage 的 aggregate 与 model map 关系，再决定是否支持补足。设计仍须维持“一 usage 一 event”，不得直接复制 C 的按模型分成多条事件。
- 验收覆盖 missing/null/0/wrong-type 的不同语义、缺 total 合法 fallback、cache clamp、reasoning 不双计、同一 eventId 的多个 usage、重复 sync 幂等。

### 5. Antigravity：参考不能替换原生 descriptor 与历史保护

#### 可以借鉴或保留

- C 用只读 SQLite connection 和 `BEGIN DEFERRED TRANSACTION` 固定读取快照，返回 `Result<Option<Vec<AntigravityUsageEvent>>>`；`None` 表示 zero-page 未初始化库，数据库打开/查询错误继续以 Err 传播。见 `C:rust/adapters/antigravity/src/parser.rs:143-171,290-331`。loader 对 empty snapshot 单独说明，见 `C:rust/adapters/antigravity/src/loader.rs:19-40`。可借鉴错误、尚未初始化、完整但无 usage 的分类。
- C 的 modern protobuf 映射与本项目核心字段一致：1=model、2=input、3=total output、4=cache creation、5=cache read、9=reasoning、10=visible output。见 `C:rust/adapters/antigravity/src/parser.rs:689-703`。仅把该映射作为交叉验证；descriptor 仍是语义依据。
- T 的 SQLite fingerprint 把 `-wal` 加为 related file，并使用 metadata/samples 避免无用途的全库 SHA-256。见 `T:crates/tokscale-core/src/message_cache.rs:402-418`。本项目已经要求观察 DB+WAL，设计须验证当前实现是否重复扫描，而不能仅因参考项目存在 cache 就再加第二套 cache。

#### 不采纳项及理由

| 参考行为 | 精确位置 | 不采纳理由 |
| --- | --- | --- |
| T 把 #1+#2 当 input、#9 当 output、#10 当 reasoning，并固定 cache_write=0 | `T:crates/tokscale-core/src/sessions/antigravity_cli.rs:312-325,374-385` | 与本项目 descriptor-proven mapping 冲突，回退会再把 model enum 当 token，并丢失 cache creation |
| T 打不开 DB 返回空 Vec；无 gen_metadata 查询采用安静路径 | 同文件 `83-115` | 空结果无法作为本项目“完整快照可提交”的证据；缺失/不可读不能清除旧历史 |
| T 从 session/mtime 回退请求时间；C 也有 file-mtime fallback | `T:crates/tokscale-core/src/sessions/antigravity_cli.rs:338-352,420-446`；`C:rust/adapters/antigravity/src/parser.rs:143-144,199-210` | 本项目只接受 typed request time；文件变更不能迁移历史用量日期 |
| C 同时遍历 direct usage 与 retry_usages | `C:rust/adapters/antigravity/src/parser.rs:199-226` | 本项目已观察 direct usage 可以是 retries aggregate；必须优先 attempts，不能假设后续 identity merge 足以消除 aggregate 双计 |
| C 缺 gen_metadata 时错误 | `C:rust/adapters/antigravity/src/parser.rs:320-331` | 本项目允许 steps-only；不能直接复制“缺 gen_metadata 即非支持库”的判断 |
| C zero-page DB 返回 None 并继续汇总其他库 | `C:rust/adapters/antigravity/src/parser.rs:161-163,290-317` | 本项目是有历史的 product-group replacement；一个已跟踪库变空不能据此自动确认为完整替换快照 |

用户日志的 `antigravity` legacy 与 `antigravity_ide` tracked DB missing 必须作为两个状态呈现。前者由 accounting marker 控制写入资格；后者由产品组输入完整性决定能否替换。二者都要求保留已存历史，但恢复条件不同。已有规范支持该区分，见 `token-accounting-contracts.md:121-150`、`source-sync-contracts.md:135-148`。参考项目不能证明“缺失 DB 只是零字节初始化文件”。

#### 子任务设计输入

- 对 source-level missing/legacy 状态使用明确 reason，不把清洗后的整句英语塞入 record sample 标识。闭集状态应包含数据是否保留、是否有新增写入、受保护的恢复动作。
- 可以研究在发现产品组已不完整后，是否存在可提前拒绝的无效重放。前置判定仍必须先完成跨 CLI/IDE 的身份与所有权发现，不能只比较目录里的文件数。该项是待测设计建议，本研究未证明当前浪费发生在哪个函数。
- 验收从正数旧历史开始，覆盖 missing、zero-byte、无两张用量表、steps-only、DB/WAL 变化、busy、decode failure、cancel、cross-product copy；断言事件、bucket、facts、cursor、marker 保留且无 repair success。

### 6. 大文件、热路径与续读：优先减少重复工作

#### C 的负载分配

`chunk_file_indexes_by_size` 读取大小，降序排序，再把下一个文件分配给累计字节最少的 worker。见 `C:rust/adapters/common/src/lib.rs:49-80`。Codex loader 使用该函数并把 worker 结果放回原始 index，保留文件结果顺序，见 `C:rust/adapters/codex/src/loader.rs:170-216`。

可采纳：当 fixture 或 profile 证实少数大文件使 worker 工作量不均时，按预计读取量分配。对 Codex 应优先用 `size - stored_offset`，对 Claude 应使用完整 project replay 的估计量；实际采用取决于当前 parser 分组边界。不得打散同项目 dedupe 或 Antigravity product atomicity。提高 parser 并发不等价于允许多个 SQLite writer。

#### T 的 fingerprint 与 Codex state

- `SourceFingerprint` 保存 size、mtime(ns)、sample hashes、full content hash 和 related files，见 `T:crates/tokscale-core/src/message_cache.rs:212-247`。hot path 校验 metadata 与 samples，见同文件 `688-709`；sample 大小 4096 bytes、最多 5 个点，见 `60-64,2995-3019`。每 watched file 的采样上限为 20 KiB，但 sidecar 数量和 cold/changed full hash 另计。
- 未缓存 Codex 文件直接解析，避免 parse 前先做一次会被丢弃的完整哈希，见 `T:crates/tokscale-core/src/lib.rs:1908-1919`。已知 consumed_offset 等于 fingerprint size 时复用 hash，见 `message_cache.rs:3122-3142`。这两项“避免重复求值”的思路可用于本项目计量结果确认后的优化。
- Codex 续读持有 parse state 并 `seek(start_offset)`，见 `T:crates/tokscale-core/src/sessions/codex.rs:1363-1396`。cache 只有在 newline boundary 才可建立，见 `message_cache.rs:3104-3118,3145-3159`。本项目已有相同边界目标，应复用测试思路而不增加另一份 durable cursor。
- **不能认定 T 的 append 为纯尾部 I/O**：`lib.rs:1966-1974` 在续读前调用 `codex_prefix_matches`，后者调用 `hash_prefix(path, consumed_offset)`，见 `message_cache.rs:3162-3169`；changed fingerprint 的 Full 模式还会计算全文件 hash，见 `3031-3067`。读更少的 JSON 不等价于读更少的文件字节。
- sample fingerprint 只能证明 sample 没变，不能证明未采样范围没变。是否接受此限制取决于本项目现有 replacement/rotation 识别契约；不能把 samples-only 当作完整内容相同的证明。

#### 计量与验收建议

采用可控临时 fixture，分别记录 metadata 枚举、fingerprint 字节、实际 JSONL 扫描字节、解析、排队/锁等待、writer 事务及 SQL 类别耗时。冷导入、热无变更、单文件少量 append、同项目单文件变更、same-size rewrite、truncate/rotate、DB/WAL-only change 分开测量。

在相同机器、同一构建 profile、相同 fixture 与相同输出校验下比较修改前后；报告 wall-clock、读取量、事务次数、events/facts 数和峰值内存。验收先要求结果/幂等/取消一致，再为已证实瓶颈设阈值。现有日志的 `PARSE=20.4s` 与 `WRITE=105.2s` 无法单独证明 CPU、I/O、锁等待或 SQLite commit 的占比。

### 7. 写入：借鉴 dirty 集合，不能替换事务协议

T 的 cache writer：

1. `state.dirty == false` 时直接返回，持有文件锁后写 cache。见 `T:crates/tokscale-core/src/message_cache.rs:2540-2579`。
2. dirty/deleted keys 各自只分组一次，随后只遍历受影响 shard，避免每 shard 再扫描全部 key。见同文件 `2582-2606`。
3. 某 shard 写成功后才清对应 dirty/deleted 状态；失败 shard 留待以后重试。见 `2675-2702`。
4. 先写临时文件、flush、fsync，再原子替换，失败只删临时文件，保留上一份 canonical cache。见 `2934-2955`。

可采纳的是限定受影响集合、避免重复分组/查询、成功提交后推进状态及失败保留旧状态的原则。本项目已有 `SyncShard` 原子协议，不能引入独立 cache 写入来替代 event+bucket+facts+cursor 的一致提交；也不能为缩短计时先提交 cursor 或跨事务删除旧 event。

C 的 Codex loader 返回内存 event vector（`C:rust/adapters/codex/src/loader.rs:150-164,219-234`）；C 的 Grok loader 返回 `Vec<LoadedEntry>`（`C:rust/adapters/grok/src/loader.rs:18-63`）。T 此处写 bincode cache shard（`T:crates/tokscale-core/src/message_cache.rs:2937-2949`）。这些路径没有构成与 llmusage SQLite materialization 相同的可比基准，不能据此宣称上游更快，也不能把其耗时作为本项目 `WRITE` 目标。

推荐把写入优化放在测量之后：先确认 `WRITE` 是否包含等待和全表/全源扫描，再分别测试重复预读、pricing lookup、logical dedupe、reset/bucket 更新、facts 与 cursor 保存。仅针对计量指出的热点优化。以上类别是待验证项，不是对本次 105.2s 的原因归因。

### 8. 可复用回归测试位置

| 参考位置 | 借鉴的用例 | 本项目验收约束 |
| --- | --- | --- |
| `T:crates/tokscale-core/src/sessions/codex.rs:2002` `test_incremental_parse_matches_full_parse_for_appended_lines` | 全量与续读等价 | 同时核对 persisted totals、facts 与 cursor，不只比较 Vec |
| `T:crates/tokscale-core/src/message_cache.rs:6156,6168,6184` | newline boundary、中间 rewrite、未采样区域 rewrite | 不放松本项目 durable offset；采样不能成为全文同一证明 |
| `T:crates/tokscale-core/src/message_cache.rs:3564,3580` | hot metadata/samples 命中及同 metadata sample 变更 | 记录真实读取字节，避免“没 parse 就没 I/O”结论 |
| `T:crates/tokscale-core/src/message_cache.rs:6468,6555` | 原子替换和独立 shard 损坏隔离 | 本项目对应 DB 事务回滚与旧历史保留，不照搬文件 cache 架构 |
| `C:rust/adapters/common/src/jsonl.rs:223,245` | marker filter 与坏 JSON | 需增加本项目诊断计数、样本隐私与取消断言 |
| `C:rust/adapters/antigravity/src/loader.rs:755,767,787,803,822,864` | zero-byte、open/query/malformed/schema error | 保留步骤表单独可用语义；从旧正数历史验证失败不清零 |

上述用例仅做源码读取，未运行参考项目测试。实施期应使用 llmusage 的测试入口，先跑 `cargo test --locked --all-features --test sync -- --test-threads=1`、相关 store/cli target，再按改动面执行 `python scripts/ci-rust.py` 或 `just ci`。规划阶段不因报告写入运行产品测试。

## Files Found

| 文件 | 作用 |
| --- | --- |
| `C:rust/adapters/codex/src/parser.rs` | 逐行筛选、累计 usage、fork replay；存在受限 reader 对比边界 |
| `C:rust/adapters/codex/src/loader.rs` | 按大小并行读取、保序合并、内存 dedupe |
| `C:rust/adapters/common/src/lib.rs` | 按累计字节分配 worker |
| `C:rust/adapters/common/src/jsonl.rs` | prefilter、typed JSONL、缺失数字默认值 |
| `C:rust/adapters/grok/src/parser.rs` | top-level/modelUsage、usage total 与 cost 的差异 |
| `C:rust/adapters/grok/src/loader.rs` | 文件错误策略与跨文件 eventId 去重 |
| `C:rust/adapters/antigravity/src/parser.rs` | 只读快照、字段映射、retry 与错误行为 |
| `C:rust/adapters/antigravity/src/loader.rs` | 空库提示与多库汇总/测试 |
| `T:crates/tokscale-core/src/lib.rs` | Codex hot/append/reparse 决策入口 |
| `T:crates/tokscale-core/src/message_cache.rs` | metadata/samples/hash、parser state、dirty shard 持久化 |
| `T:crates/tokscale-core/src/sessions/codex.rs` | stateful seek 续读与 additive token 表示 |
| `T:crates/tokscale-core/src/sessions/grok.rs` | usage 字段默认值、fallback 混合与 record identity |
| `T:crates/tokscale-core/src/sessions/antigravity_cli.rs` | native DB 读取及与本项目相冲突的 token/time mapping |

## Caveats / Not Found

- 本研究不读取真实用户日志，不确认这 13 条 Codex record 的内容，也不确认 13 条 Grok issue 的具体缺字段组合。以主任务的脱敏诊断为准。
- 本研究未执行任何 sync、rebuild、benchmark、参考项目测试或 Git 操作。两个提交锚点来自主会话的更新结果；未读取其他会话 active-task 状态。
- 尚无相同数据集、数据处理范围和运行条件的三项目可比性能基准。不能给出 ccusage/tokscale 相对 llmusage 的速度排名或收益百分比。
- 安装二进制与当前工作区的版本差异由主任务 `prd.md` 记录。本报告用当前工作区规范作为设计边界，不声称已证明安装二进制每一条代码路径。
- 参考项目缓存可重新生成，但 llmusage 缺源历史可能无法重建。不能用 cache eviction、静默空结果或 parser-version 冷启动替代显式历史修复。
- ADR 0006/0007 含历史拟稿内容；本报告的当前行为约束优先采用 source-sync/token-accounting specs，不把旧 ADR 的示例实现当作当前代码事实。
