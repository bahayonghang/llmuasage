# DeepSeek Harness（DSH / dsh）调研

调研日期：2026-08-16。**二次校验修订（关键）**：初稿与审阅报告都只查了 `~/.deepseek/`（旧根，sessions 为空）并据此判定"无样本、命中停止规则"。二次核查发现真根是 **`~/.dsh/`**——本机有 **15 个真实会话样本（5 空 + 10 正常）且含完整 usage 记录**，官方 token 语义文档齐备。停止规则不再适用，DSH 升级为可实现的解析器任务（zstd 解码是主要工程决策）。

## 1. 产品背景

- DeepSeek Harness（`dsh`）：DeepSeek 开源的 MIT 协议 TypeScript agent harness（~45 万行、~219 workspace 包、vendored Cordis）。官方入口 `deepseek.com/harness`。
- "一切皆插件"：models、tools、skills、**sessions**、sandboxes、storage、loops、scheduling、UI 均可替换——被动解析必须**钉死默认 session 插件的落盘格式**（JSONL 后端），插件换掉 persistence 时自然失效，候选表须注明。

## 2. 落盘合同（官方文档 + 本机实测一致）

- **根：`~/.dsh/`**（官方 Data and Privacy 页明确 "All session logs live locally in ~/.dsh/sessions/"）。本机 `~/.dsh/` 含 `sessions/ storages/ profiles/ settings.yaml cordis.patch.yml pet.json .credentials.yaml .agent-presets/`。
- **布局**：`~/.dsh/sessions/--<normalized-cwd>--/<id>/session.jsonl.zstd`。本机实例：
  - `--D-Documents-Code-Github-ccr--/session-60249868-a5a4-…/session.jsonl.zstd`（主会话，id 带 `session-` 前缀）
  - `--D-Documents-Code-Github-ccr--/798efa4a-64d3-…/session.jsonl.zstd`（裸 uuid，子代理）
- **压缩**：默认 zstd 压缩；`compression: 'none'` 配置可关（`session.jsonl`）。**多帧**：独立 zstd 帧拼接（CC2DSH 插件文档：帧 1 恰好一条 session 记录——本机实测证实）。
- 官方 telemetry env：`DSH_TELEMETRY_OTLP_URL` / `DSH_TELEMETRY_DISABLED`（privacy 页）。**无官方存储根覆盖变量**（privacy 页未列）；tokscale 用 `DSH_HOME` 指向 `~/.dsh`——llmusage 沿用 `DSH_HOME` 命名（与 tokscale 一致，且为 llmusage 自造覆盖时须注明）。
- 本机 `~/.deepseek/`（2026-05 的旧痕迹：config.toml/audit.log/skills，sessions 空）与 `~/.dsh/`（2026-08-14 起）并存——DSH 根发生过迁移；monitor 可同时探测两个根，解析器只认 `~/.dsh`。
- SQLite 会话后端：审阅提及官方支持，本机未见痕迹（`storages/` 下只有 `session_projcache.json`/`workspace.json`）——按未证实记录，首版不覆盖。

## 3. 会话记录词表（本机最大会话实测：3.2MB 压缩 → 9MB / 9969 行）

`session`（头）、`subagent/descriptor`、`session/end-seed`、`sandbox/mode`、`approval/policy`、`permission/preset`、`activity/status`、`agent/inbox/spliced`、`turn/start|end`、`step/start|end`、`user/message`、`session/title`、`request/header`、`request/context`、`assistant/chunk`、`reasoning-chunks`、`text-chunks`、`tool-call-chunks`、`assistant/message`、`tool/call`、`tool/result`、`todo/write`。

关键字段：

- **会话头**（每文件第 1 帧）：`{type:'session', version: 0, id, createdAt(ms), cwd, origin?('subagent'), delegationDepth, parentSession?, agentPreset?}`。`version: 0` = SESSION_FORMAT_VERSION v0，**无兼容承诺**（解析器按字段容错 + parse issue 观测漂移）。
- **usage**：`assistant/message` 的 `data.usage` 与某条 `assistant/chunk` 的 `data.chunk.usage` **双带同值**（实测 328 = 164×2，逐条相等）→ 解析器只读 `assistant/message`（每 step 恰一条），天然去重。
- **TokenUsage**（官方 llm-streaming 文档 + 实测字段一致）：`{inputTokens, outputTokens, cacheReadTokens, reasoningTokens}`
  - `inputTokens` 是**未缓存 input**（与 cache 分离）：实测 `input=7619 < cacheRead=19840`。
  - `reasoningTokens` **已含在 `outputTokens` 内**（官方明确 total 不得再加 reasoning）。
  - 本机样本无 `cacheWriteTokens`、无 `totalTokens` 字段（官方 schema 有 cacheWrite，出现与否随 provider）。
  - 归一化到 llmusage：`input = inputTokens`、`cache_read = cacheReadTokens`、`output = outputTokens`（含 reasoning）、`reasoning = reasoningTokens`（诊断）、`total = input + cache_read + output`（求和，候选表注明无权威 total）。
- **模型身份**：**优先 `assistant/message.data.message.source.{provider, model}`**（本机 835/835 全有，例 `deepseek-official` / `deepseek-v4-flash`）；`request/header.data.header.config.{provider, model}` 作消息缺 source 时的就近兜底。
- **fork 种子**：会话头可带 `seedLength`（本机暂无）——fork 把父前缀原样复制进子会话，`seq < seedLength` 的行必须跳过，详见 §5.1。
- **时间戳**：记录顶层 `time`（ms epoch）+ `seq` 递增序号（event_key 可用 `seq`）。
- **空会话**（5 个实测）：仅 5 条状态记录（session/permission/sandbox/approval/activity），零 usage → 解析器干净跳过。
- **中断容错**：多帧 zstd 尾帧截断时解压循环在残帧处停止（本机解码循环天然如此）→ durable boundary 语义良好。

## 4. 样本清单（15 个，2026-08-16）

5 个空会话（0 条 usage）+ 10 个正常会话（293KB~3MB 压缩，19~180 条 assistant/message，**每条恰好 1 个 usage**）。**interrupted 类**：本机 15 个文件逐个扫描均无撕裂尾帧。解析器阶段用合成多帧 zstd（完整帧 + 截半尾帧）覆盖 durable-boundary 合同；未在本机 kill 正在写入的真实 dsh 会话，因此真实撕裂样本仍缺，合成 fixture 已进入 `src/parsers/dsh.rs` 与 `tests/sync_regression.rs`。

## 5. 参考实现（重要更新：ref 快照已含 DSH 支持）

ref/repo/tokscale 已更新至含 DSH 的提交（`59712ada feat(clients): add DeepSeek Harness support (#1126)`）。**tokscale 现在有完整的 DSH 解析器可直接对照**：`crates/tokscale-core/src/sessions/dsh.rs`（635 行 + 13 个测试，含官方快照数值）。ccusage 仍无 DSH 适配器。

### 5.1 tokscale dsh.rs 实现要点（已逐行核对）

**发现契约**（`clients.rs` + `scanner.rs:504-516`）：
- `ClientId::Dsh = 46`，root `PathRoot::EnvVar { var: "DSH_HOME", fallback_relative: ".dsh" }`，relative `sessions`。
- 扫描 pattern `dsh-session-log`：**文件名精确等于 `session.jsonl.zstd` 或 `session.jsonl`，`sessions/` 下任意深度**（不固定两层）；同目录其它 zstd/jsonl 文件排除。

**zstd 处理**：
- 用 `zstd` crate（C 绑定）的 `stream::read::Decoder` **流式解码**，不是一次性 `decode_all`。
- **按帧魔数分派**（`0x28 B5 2F FD`，RFC 8478）而非文件扩展名：`compression: none` 后端把同样的行写进同名 `session.jsonl`，两种拼写都是会话日志。
- **撕裂尾帧恢复**：活跃会话每次 flush 追加一个 zstd 帧，扫描撞上正在写的文件时尾帧不完整——解码器 `Err` 时保留已解出的前缀（与 DSH 自带 reader `readZstdPrefix` 行为一致），`lossy_lines` 再丢弃不完整的末行。一次性解码会把整个会话报成 0 token（有非空测试断言 `decode_all` 确实失败）。这正是 llmusage durable-boundary 语义。

**记录解析**：
- 只读 `assistant/message` 的 `data.usage`（与我的本机实测一致：chunk 双带同值不读）。
- **每条消息自带 `data.message.source.{kind, provider, model}`**（本机 835/835 全有，例 `{"kind":"model","provider":"deepseek-official","model":"deepseek-v4-flash"}`）→ 模型归属优先用它，`request/header.data.header.config` 只作兜底（消息缺 source 时用最近一条 header 路由）。
- 会话头 `session`：`id/createdAt/cwd/seedLength`；**目录名是 session id 的兜底**（头缺失时取文件父目录名）。
- 跳过规则：`usage` 全零（噪声行）；顶层 `time` 缺失或 ≤ 0。

**fork/种子语义（本机未遇但官方快照证实，双计风险）**：
- DSH fork（子代理/resume）把父会话的完成前缀**原样复制**进子会话记录（同 `seq`/`time`/`usage`/`message.id`），并在子会话头记录 `seedLength`（继承的事件数）。
- 双重防线：① `seq < seedLength` 的行直接跳过（头带 seedLength 时）；② 去重键以 `data.message.id`（每次调用的 `crypto.randomUUID()`，fork 原样复制）为基——丢失 seedLength 的 resume/重导出转录里，父子的同一行在不同文件也共享同一个 key，跨文件去重可折叠。本机 835 个 message.id 全部唯一、暂无 seedLength 会话，防线照做。

**去重键形状**：`dsh:{msg:<message.id>|sid:<session_id>}:{time}:{provider}:{model}:{input}:{output}:{cache_read}:{cache_write}:{reasoning}`——id 缺失时退 `sid:` 前缀，且时间/路由/token 全进键（DSH 脱敏快照会把 message.id 洗成全文件同一个占位符，只按 id 折叠会吞掉不同调用）。

**token 语义**（官方源码引用 + 快照数值双证）：
- `inputTokens` 未缓存 input（DeepSeek 适配器落盘前已从 prompt_tokens 减掉缓存，`llm-deepseek/src/translate.ts`）。
- `reasoningTokens` = `completion_tokens_details.reasoning_tokens`，是 `outputTokens`（= `completion_tokens`）的**子集**；DSH 自己的 meter 就是 `input + cacheRead + cacheWrite + output`，不加 reasoning（`llm/token-meter/src/index.ts`）。
- `cacheWriteTokens` → cache write 通道（本机样本未出现，schema 有）。
- tokscale 把 output 减掉 reasoning 再入桶，因为它的 `TokenBreakdown` 五桶**可加**且计价对 output/reasoning 同价（不减会双计）；**llmusage 通道语义不同**（reasoning 是诊断通道、不计 total），所以 llmusage 保留 `output = outputTokens` 原样、total = `input + cache_read + cache_creation + outputTokens`——与 DSH 官方 meter 一致。

**turn-start 标记**（`data.turn` 首见或 user/message 触发）：tokscale 的会话分析维度，llmusage v1 不需要。

### 5.2 其余相关提交（ref 更新）

- `d8b23d14` zcode 只读 SQLite 打开抽公共 helper（#1093），无语义变化。
- `416721e2`/`2b45f576` antigravity_cli：缺 responseModel 的 turn 归属 + SessionModels 先过 alias 再判歧义——antigravity 子任务 design 已覆盖同款规则。
- `43d4abdb`（core：invalid JSONL 字节后保留后续记录）与 llmusage `BoundedJsonlReader` 的 malformed-line 容错语义同向。

## 6. 来源身份与状态（吸收审阅修正）

- **不加 parser=false 的 SourceKind**：`source_status.rs:174-179` 规定无 parser 的 SourceKind 一律显示 `historical_only`（不是 blocked；blocked 只存在于 platform monitor）。且 stable id `deepseek` 会与 provider/模型名（本机 zcode 已有 `deepseek-v4-flash`）混淆。
- 登记路径：platform monitor `platform_id = "deepseek_harness"`、`source_kind = None`（同 gemini/reasonix 模式），roots `~/.dsh`（+ 探测旧根 `~/.deepseek`），parser_status 先 `Planned`（样本已到位）；**解析器落地同一个任务内做**，`SourceKind::DeepseekHarness`（stable id `deepseek_harness`）与 parser 同 PR 注册，避免出现 historical_only 空状态。
- sync 在解析器注册前不写任何 usage 行。

## 7. 主要工程决策：zstd 解码

| 选项 | 说明 | 倾向 |
| --- | --- | --- |
| `zstd` crate | C 绑定，流式 `Decoder` 天然支持撕裂尾帧前缀恢复；**tokscale 同款且其 Windows CI 已验证**（ref 快照 dsh.rs 实测用法） | **首选**（跟随参考实现） |
| `ruzstd` crate | 纯 Rust 零 C 依赖，性能较慢（文件 ≤3MB 可接受）；流式解码需确认撕裂帧行为 | 备选（CI 契约拒绝 C 依赖时） |
| 只支持 `compression: 'none'` | 默认配置读不了，不可接受 | 否 |

依赖引入须过 `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md` 审查（implement 清单第一道 gate）。

2026-08-16 落地结论：`Cargo.toml` 增加 `zstd = "0.13.3"`。`cargo clippy --all-targets --all-features -- -D warnings` 与 `cargo test --all-features -- --test-threads=1` 通过。隔离 `CARGO_TARGET_DIR=target-msrv-dsh` 的 `cargo +1.95.0 check --locked --all-features` 通过。

## 8. cursor 与去重策略

- 首版（全量模式）：**fingerprint（size+mtime+tail signature）变化 → 全文件重解压重解析 + event_key 幂等**（与 antigravity 子任务同策略；文件 ≤3MB 可接受，帧边界增量解压记为优化项）。
- **event_key 恒为复合键**（三次校验修订）：`message.id（或 sid 兜底）+ time + provider + model + 全部 token 通道`一起进键。仅"id 缺失才复合"无法处理非空但重复的脱敏占位 id（tokscale dsh.rs:207-220 全字段进键的原因）；fork 拷贝行与父行全字段一致 → 键相同跨文件折叠（丢失 seedLength 的 resume/重导出也能折叠）。
- **seedLength 守卫**：会话头带 `seedLength` 时跳过 `seq < seedLength` 的行（fork 种子前缀）。
- **跨文件所有权（会话家族重放）**：`usage_event.event_key` 是全局主键（`migrations.rs:264`）而 reset 按单文件 `source_path_hash` 删除（`sync_writer.rs:196`）——fork 折叠的共享键只归属首插文件，owner 重写移除事件而未变化副本被 cursor 跳过时事件会消失。对策：盘点期建 `session→file` 与 `parentSession→child files` 映射，文件变化 reset 时**强制重放同家族成员**（即使 fingerprint 未变）；测试覆盖"owner 重写、duplicate 未变化"。
- **bounded run**：`--recent-days` 不推 cursor、不执行整文件 reset/家族重放；窗口过滤按记录 `time`（契约 `source-sync-contracts.md:86-88`）。
- 隐私：会话日志含完整 prompt/工具结果（官方明示）→ 只读 `assistant/message.data.usage`、`data.message.source` 的 provider/model 标量、顶层 `time/seq`、会话头 `id/createdAt/cwd/seedLength/version`（cwd 哈希化）；fixture 一律合成。
- **流式与资源上限**：文件 → 流式 Decoder → 行分割管道（4 MiB 单记录上限 + partial-tail），不得整体物化解压结果（绕过 `DEFAULT_MAX_JSONL_RECORD_BYTES` 会破坏 bounded 契约）。

## 9. 解锁条件（旧版作废）

~~"无样本命中停止规则"~~ → 已解除。剩余 gate：zstd 依赖评审（§7，真实 manifest 变更 + MSRV 证明）+ **interrupted 真实样本采集**（§4，三类样本补齐）+ 脱敏 fixture。满足后本任务内直接实现解析器。

## 10. 来源

- [DeepSeek Harness 官方 Data and Privacy 文档](https://deepseekdocs.com/en/docs/user-guide/privacy)（`~/.dsh/sessions/`、zstd、内容范围、telemetry env）
- [DeepSeek Harness Quickstart](https://deepseekdocs.com/en/docs/getting-started/quickstart)
- [官方 llm-streaming 文档（TokenUsage 语义）](https://ithub.global.ssl.fastly.net/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/llm-streaming.md)
- [We Read DeepSeek Harness (Developers Digest)](https://www.developersdigest.tech/blog/deepseek-harness-dsh-first-look)
- [DeepSeek Harness 官方页](https://deepseek.com/harness)
- [tokscale（DSH 支持说明）](https://github.com/junhoyeo/tokscale)
- [dsh-session-audit 插件](https://awesome-dsh-plugin.com/p/bwndlct/dsh-session-audit/) · [CC2DSH（帧结构）](https://www.dshplugin.store/plugin/Seafood-Y/CC2DSH)
