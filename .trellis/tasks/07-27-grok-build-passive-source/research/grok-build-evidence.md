# Grok Build 用量数据源调研证据

调研时间：2026-07-27。结论：**Grok Build（`grok` CLI 编码代理）可作为被动源接入，但 token 语义为 `total_only`，且本机当前版本（0.2.112）的覆盖度有限。**

## 参考实现对比

### ccusage（`ref/repo/ccusage`）

- **不支持 Grok CLI 作为数据源**。`docs/guide/source-support-qa.md:32-33`：调查过 Grok CLI，其本地 SQLite 无可用 token 记账（无 token 数、无模型、无成本），因此放弃。
- grok 相关代码仅是：pricing 里的 `grok-4.3` 价格/上下文行（`rust/crates/ccusage/src/pricing.rs`），以及 droid/codebuff/hermes 适配器里把 `grok*`/`xai/*` 模型名归一到 `xai` provider 的逻辑。
- 注意：ccusage 调查的是旧的 "Grok CLI"（SQLite 存储），与现在的 Grok Build（JSONL 会话目录）**不是同一个产物形态**。

### tokscale（`ref/repo/tokscale`）

支持 Grok Build，核心实现在 `crates/tokscale-core/src/sessions/grok.rs`，注册在 `crates/tokscale-core/src/clients.rs:431`（`GROK_HOME` 环境变量，回退 `~/.grok`，扫描 `sessions/**/updates.jsonl`）。

数据布局：`~/.grok/sessions/<urlencoded-workspace>/<session-id>/`，含：

- `updates.jsonl` — JSON-RPC session update 流。tokscale 从 `params._meta.totalTokens`（多个候选路径）读**累计总 token 计数器**，无 input/output 拆分。
- `signals.json` — 会话汇总：`totalTokensBeforeCompaction`、`contextTokensUsed`、`totalTokens`、`primaryModelId`、`modelsUsed`。
- `summary.json` — `current_model_id`、`created_at`/`updated_at`、cwd、git 信息。
- `events.jsonl` — 运行事件（MCP 启动等），可提供 `model_id`/`session_id`/`ts` 兜底。

tokscale 的解析策略（值得照抄的坑）：

1. **按轮切分**：`sessionUpdate == "user_message_chunk"` 开启新轮，轮内取 `totalTokens` 最大值减基线得增量，全部记为 input（无拆分）。
2. **计数器单调化**：流式工具更新会重复/回退 totalTokens，小于前值的直接丢弃。
3. **signals.json 对账**：`effective_total = max(totalTokens, totalTokensBeforeCompaction + contextTokensUsed)`，超出 updates 已计部分的差额补一条 `grok:<session>:signals` 去重键的对账事件——否则 compaction 过的会话严重少算（测试里有 320 万 token 的差额案例）。
4. **对账事件时间戳锚定到最后一条 update 的时间，而非 signals.json 的 mtime**——mtime 在活跃会话中不断刷新，会导致每次重扫把几百万 token 的差额迁移到新的一天。
5. workspace key 是 **URL 编码的路径**，需 percent-decode（Windows 上形如 `D%3A%5CDocuments%5C...`）。
6. 无模型信息时回退 `grok-unknown`，provider 固定 `xai`。

### tokscale 定价侧

`pricing/aliases.rs`、`pricing/lookup.rs` 有 grok 模型别名与价格归一；llmusage 需确认自己的 pricing catalog 是否覆盖 `grok-4.5` 等 model id。

## 本机真实样本（Grok Build 0.2.112，2026-07-27）

- `~/.grok/sessions/` 下 2 个 workspace、8 个会话，均含 `updates.jsonl`；最大 3.7MB。
- **本机版本的 `updates.jsonl`、`chat_history.jsonl`、`events.jsonl`、`summary.json` 全部没有任何 token 字段**（逐文件 grep 验证）。tokscale 依赖的 `_meta.totalTokens` 在 0.2.112 不落盘（tokscale 测试样本应来自其他版本/平台）。
- **仅 1/8 会话有 `signals.json`**，含 `contextTokensUsed: 77642`、`totalTokensBeforeCompaction: 0`、`primaryModelId: "grok-4.5"`、`modelsUsed: ["grok-4.5"]`、`contextWindowTokens: 1000000`。signals.json 疑似仅在会话正常结束/达到某条件时写出。
- `summary.json` 稳定提供 `current_model_id`（样本值 `grok-4.5`）、`created_at`/`updated_at`、`agent_name`（如 `grok-build-plan`）、cwd/git 元数据。
- 更新中的 update 行样例：`{"timestamp":1785042463,"method":"_x.ai/session/update","params":{...,"update":{"sessionUpdate":"hook_execution",...},"_meta":{"eventId":"...","agentTimestampMs":1785042463617}}}` — method 名为 `_x.ai/session/update`（tokscale 测试样本里是 `session/update`，解析不应依赖 method 名）。
- ⚠️ 工程坑：会话目录下有 `terminal/` 子目录，内含**阻塞读取的特殊文件**（递归 grep 卡死 5 分钟）。扫描器必须只按精确文件名（`updates.jsonl`/`signals.json`/`summary.json`）定位，绝不能递归读会话目录下的任意文件。另有 `.lock` 空文件（`updates.jsonl.lock` 等），需跳过。
- 隐私：`chat_history.jsonl`、`system_prompt.txt`、`prompt_context.json` 含完整对话/提示词，解析器不得读取；`summary.json` 含 git remote URL 与本地路径，仅持久化归一化后的 workspace 标签所需最小字段。

## 对 llmusage 接入的结论

1. token 质量只能标 **`total_only`**：无 input/output/cache 拆分；tokscale 把总量记 input，llmusage 应按自身 token-accounting 契约决定归入哪个通道。
2. 必须同时实现两条取数路径：updates.jsonl 逐轮增量（新版本 Grok 有效）+ signals.json 会话级对账（本机当前版本唯一有效路径）。两条路径都无数据的会话（本机 7/8）合法地产出 0 事件。
3. 最终采用**会话原子重放**：每个 sidecar 的 `FileCursor` 仅用于指纹/mtime/size 变化检测，不做 offset 续读；任一 sidecar 变化就按共享 session path hash 删除并重插整会话事件。signals 对账键保持 `grok:<session>:signals`，时间戳锚定最后活动而非 mtime。该裁定取代早期“updates offset + signals 快照差额”的混合建议，理由见下方外部审核第 2/3 条。
4. 通过被动接入门禁（`docs/agents/passive-parser-onboarding.md`）所需证据已具备：真实样本（正常/空/中断会话均有）、token 语义（total_only + signals 对账）、发现规则（`GROK_HOME` 回退 `~/.grok`，`sessions/*/*/updates.jsonl`）、隐私边界（见上）。

## 附录：2026-07-27 外部审核（Codex）校验记录

逐条对照源码核验，5 条主发现与 2 条契约表述全部属实，已吸收进 prd/design/implement 修订：

1. **计价接口不读 total** ✅ `src/query/pricing.rs:52` `CostTokens` 仅含 input/cache_read/cache_creation/output/reasoning；`sync_writer.rs:331` 按子通道喂入。grok 子通道全 0 + 价格行存在 ⇒ 成本恒 $0 的错误精确性。→ 裁定 MVP unpriced，不加价格行。
2. **insert-only 与固定对账键冲突** ✅ `sync_writer.rs:316` `INSERT OR IGNORE`，`event_key` 唯一。→ 改会话原子重放。
3. **turn_index 续读碰撞** ✅ `store/mod.rs:52` `FileCursor` 只存 last_total/model，无轮序号。→ 放弃 offset 续读，全量重放下 turn_index 稳定。
4. **WalkDir 全递归会进 terminal/** ✅ `source_files.rs:132` 先递归后按文件名过滤。→ grok 专用固定两级枚举。
5. **真机验收污染默认库** ✅ `--home <PATH>` 存在（`commands/help.rs:370`，覆盖 LLMUSAGE_HOME 与 `~/.llmusage`）。→ 验收用临时 home 隔离。
6. **total_only 是描述符标签** ✅ `source_descriptor.rs:69` `UsageQuality::TotalOnly` 挂在 `SourceDescriptor.quality`，非 UsageEvent 字段。→ PRD 措辞已改。
7. **provider_label 是 CCR 中继归属** ✅ `models.rs:176`。→ 保持空串，不落 `xai`。
8. **base_branch 分叉** ✅ task.json 原 `main`，工作分支 `dev`。→ 已改 `dev`。

关键新佐证：`sync_writer.rs:112` `reset_file_events_batch_tx`（`:563` 事务内调用）是现成的"按 (source, source_path_hash) 删除 + 聚合桶回退"重放协议，claude/codex/kimi_code 解析器均通过 shard 的 `reset_path_hashes` 使用——会话原子重放无需新增 store 能力。
