# Design：deepseek-harness (dsh) 被动解析器

依据：父任务 `research/deepseek-harness.md`（官方合同 + 15 样本实测 + **tokscale `sessions/dsh.rs` 参考实现逐行核对**，ref 提交 `59712ada`）；pi/grok 文件型解析器与 antigravity 子任务的 DB 重解析策略作结构范本。

## 1. 组件与边界

```
src/domain/models.rs             SourceKind::DeepseekHarness（stable id deepseek_harness，与 parser 同 PR）
src/domain/source_descriptor.rs  SOURCE_DESCRIPTORS += deepseek_harness（Precise / LocalArtifacts / parser+probe）
src/domain/platform_monitor.rs   PLATFORM_MONITORS += deepseek_harness（roots ~/.dsh + 旧根 ~/.deepseek 探测，env DSH_HOME）
src/registry.rs                  registered_parsers() += DeepseekHarnessParser
src/parsers/dsh.rs               解析器主体（zstd 解码 + JSONL 行解析）
src/parsers/source_files.rs      list_dsh_session_files()（精确文件名，任意深度）
tests/sync_regression.rs         seed_dsh + 集成测试组
Cargo.toml                       + zstd（首选）——须过 ci-toolchain-contracts 审查
```

## 2. 数据流

1. **发现**（对齐 tokscale `scanner.rs:504-516` 的 `dsh-session-log` 契约）：`$DSH_HOME`（默认 `~/.dsh`）下 `sessions/` **任意深度**、文件名精确等于 `session.jsonl.zstd` 或 `session.jsonl` 的文件；同树其它 zstd/jsonl 排除。返回 `SourceFileListing`。
2. 每文件：**流式读取**（`BufReader`，不整体载入内存）→ fingerprint 判定（`decide_file_replay`：不变则 skip）→ 变化则 `reset_path_hashes`（仅全量模式）+ 重解析。
3. **解压分派按帧魔数**（`0x28 B5 2F FD`），不按扩展名：`.jsonl` 可能装着压缩载荷，反之亦然（`compression: none` 写同名 `session.jsonl`）。zstd 用 `zstd::stream::read::Decoder` **流式管道**：文件 → 流式 Decoder → 行分割器，行分割器接入与 `BoundedJsonlReader` 等价的**单记录 4 MiB 上限**（`DEFAULT_MAX_JSONL_RECORD_BYTES`）、partial-tail 丢弃与取消语义；**尾帧撕裂（活跃写入竞争）时保留已解前缀**——即 llmusage 的 durable-boundary 语义（tokscale 有非空测试证明一次性 `decode_all` 会整会话报零）。**不得**把压缩文件或解压结果整体物化进内存（初稿"读全部字节"绕过了记录上限，作废）。
4. 行解析：`type == "session"` 记 `id/createdAt/cwd/seedLength/version`（**并登记 session→文件、parentSession→文件的家族映射**，供 §3 所有权重放用）；`type == "request/header"` 记就近 provider/model 兜底；`type == "assistant/message"` 产出事件。
5. `UsageEvent` 构造：

| 字段 | 来源 |
| --- | --- |
| `source` | `deepseek_harness` |
| `event_key` | `deepseek_harness:<hash(identity + time + provider + model + input + output + cache_read + cache_creation + reasoning)>`，其中 `identity = data.message.id`（非空时）否则 `sid:<session_id>`——**身份字段永远与 time/routing/token 通道一起进键**（对齐 tokscale dsh.rs:207-220：非空但重复的脱敏占位 id 需要其余字段分离不同调用；初稿"仅 id 缺失时复合"会把占位符 id 的不同调用折叠掉，作废） |
| `event_at` | 记录顶层 `time`（ms→RFC3339）；缺失或 ≤ 0 的行跳过 |
| `session` | 会话头 `id` 哈希；头缺失 → **文件父目录名**（tokscale 同款兜底） |
| project | 会话头 `cwd` 哈希 |
| `model` / provider | **`data.message.source.{model, provider}` 优先**（本机 835/835 全有）；缺 source → 最近 `request/header` 路由；再缺 → `dsh-unknown` |
| tokens | `input = inputTokens`（不含 cache）、`cache_read = cacheReadTokens`、`cache_creation = cacheWriteTokens`（缺省 0）、`output = outputTokens`（**含 reasoning，原样保留**）、`reasoning = reasoningTokens`（诊断）、`total = input + cache_read + cache_creation + outputTokens`（= DSH 官方 meter 口径） |
| cost | `Unpriced`（catalog 无条目） |

6. **跳过规则**：`seq < seedLength` 的行（fork 种子前缀，双计防线一）；usage 全零（噪声行）；`time` 缺失/非正。
7. shard 提交 + `FileCursor` finalize；会话头 `version != 0` → parse issue 计数（漂移观测）。

### 2b. 跨文件所有权：会话家族重放（P1 修订）

**问题**：`usage_event.event_key` 是全局主键（`migrations.rs:264`），但 replay/reset 按 `source + source_path_hash` 删除（`sync_writer.rs:196`）。fork 折叠的共享键只归属**首次插入**它的文件；若该文件被重写且移除了该事件，而另一份副本文件未变化、被 cursor 跳过，事件会消失。

**对策：会话家族重放**。解析器在盘点阶段从每个文件的第一帧 `session` 头建立两张映射：`session_id → file`、`parent_session → [child files]`。当文件 F 的 fingerprint 变化需要 reset 时，除 F 外**强制重放同一家族的其它成员**（父 + 经 parentSession 链接的子，忽略其 fingerprint 未变），使共享键的删除-再插入在同一次 sync 内闭环。DSH 家族 = 一个父会话 + 其子代理 fork，规模小（≤ 数个文件、每个 ≤3MB），重放成本可接受。若两份副本内容分叉（同 message.id 不同 token——fork 语义下不应发生），首插者胜出并记 parse issue。

### 2c. bounded run（--recent-days）契约

- `recent_cutoff` 有值时：按记录 `time` 过滤事件；**不推进全历史 cursor、不执行整文件 reset/家族重放**（契约 `source-sync-contracts.md:86-88`）；随后的全量 sync 必须能恢复窗口外历史。fingerprint 变化的文件在 bounded 模式下只按窗口解析新增事件。

## 3. 关键决策与理由

- **event_key 恒为复合键（修订）**：`identity（message.id 或 sid 兜底）+ time + provider + model + 全部 token 通道`一起进键——tokscale dsh.rs:207-220 同款。仅"id 缺失才复合"无法处理**非空但重复**的脱敏占位 id（DSH 脱敏快照把 message.id 洗成全文件同一占位符）。fork 折叠仍有效：fork 拷贝行 time/routing/tokens 与父行完全一致 → 键相同。
- **跨文件所有权 = 会话家族重放（§2b）**：全局 event_key + 按 path reset 的组合要求"共享键的文件们"作为一个重放单元，否则 owner 文件重写会让未变化的副本文件失去事件。测试必须覆盖"owner 重写移除事件、duplicate 未变化"场景。
- **fork 双计防线**：① `seq < seedLength` 跳过（头带 seedLength 时）；② 键含 message.id（丢失 seedLength 的拷贝跨文件折叠）。
- **只读 `assistant/message` 不读 `assistant/chunk`**：实测 328 = 164×2 双带同值，message 每 step 恰一条，天然去重。
- **input 不减 cache**：与 zcode 相反——DSH 官方语义 `inputTokens` 本就是未缓存 input（实测 7619 < cacheRead 19840）。**两个来源归一化方向相反，测试用各自实测数值锚定**。
- **output 不减 reasoning**：官方明确 reasoningTokens 是 outputTokens 的子集，DSH 自带 meter 即 `in+cache+out`；契约规定 reasoning 是诊断通道。tokscale 减 reasoning 是因为它五桶可加且同价计费——llmusage 通道语义不同，不减（design §5 测试锚定 `2885+25` 官方快照数值）。
- **`zstd` crate 优先**（修订：初稿倾向 ruzstd）：跟随参考实现（流式 Decoder 撕裂帧前缀恢复有现成模式与测试），tokscale Windows CI 已验证该依赖可构建；ruzstd 降为 CI 契约拒绝 C 依赖时的备选。
- **流式 + 记录上限（修订）**：不得整体物化解压结果；行分割接入 4 MiB 单记录上限与 partial-tail 语义（与 `BoundedJsonlReader` 等价），防超大记录打爆内存。
- **全文件重解析（全量模式）**：帧边界增量（记住已消费帧数）记为优化项；首版正确性优先。bounded 模式不重放（§2c）。
- **stable id `deepseek_harness`**：避免与 provider/模型名 `deepseek`（zcode 已有 `deepseek-v4-flash`）混词；与 platform_id 一致。
- **turn-start 不进 v1**：tokscale 的 `is_turn_start`（turn 首见/user-message 触发）是其会话分析维度，llmusage UsageEvent 无此槽位。

## 4. 隐私边界

- 会话日志含完整 prompt/工具结果（官方明示）→ 读取仅限：`assistant/message.data.usage`、`data.message.source` 的 provider/model 标量、顶层 `time/seq/type`、会话头 `id/createdAt/cwd/seedLength/version`、`request/header` 的 config 标量。其余字段一律不解析（窄结构体 + 忽略未知字段）。
- 持久化：usage 通道、model、session/cwd 哈希、时间戳。
- fixture：合成记录（含双带 usage 的 chunk+message 对、占位符 message.id、seedLength 父子对）。

## 5. 测试设计

- 单测（`src/parsers/dsh.rs`）：行解析（usage 提取、双带去重、空会话、全零跳过、time 缺失跳过、version≠0 计数）；归一化数值锚定（本机实测 7619/19840/171；官方快照 2885/25/23 → total 2910、reasoning=23）；帧魔数分派（.jsonl 名字装压缩载荷 / .zstd 名字装明文）；**撕裂尾帧前缀恢复**（拼多帧后截半尾帧，断言已提交帧仍计数）；**超大单记录被 4 MiB 上限丢弃并计 issue**；**fork 语义**（seedLength 跳过 + 无 seedLength 时跨文件键折叠）；**占位符 message.id 靠 time/token 分离**。
- 集成（`tests/sync_regression.rs`）：`seed_dsh()` 写合成 `.jsonl` 与压缩 `.zstd`（多帧、含截断尾帧）；sync-twice / append（追加新帧重解析幂等）/ rewrite / 删除保历史 / missing root（monitor 探测旧根 `~/.deepseek`）/ `DSH_HOME` 覆盖 / fork 父子双文件不双计；**所有权重放**（owner 文件重写移除共享事件、duplicate 未变化 → 家族重放后事件不消失）；**bounded run**（不推 cursor、不 reset/家族重放、窗口外行留给全量恢复）。

## 6. 风险

| 风险 | 缓解 |
| --- | --- |
| SESSION_FORMAT_VERSION v0 无兼容承诺 | 窄结构体 + 未知字段忽略 + `version`/字段缺失 parse issue 观测；字段大改时 issues 计数骤增可见 |
| zstd C 依赖构建问题 | tokscale Windows CI 已验证；评审 gate 不通过再退 ruzstd（流式撕裂帧行为需先验证） |
| fork/种子语义随版本变化 | 双防线（seedLength + message.id 键）互补；官方快照数值进测试锚定 |
| 根迁移史（~/.deepseek → ~/.dsh） | monitor 双根探测；解析器只认 `~/.dsh`/`DSH_HOME` |
