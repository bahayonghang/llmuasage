# PRD：deepseek-harness (dsh) 被动解析器（登记 + zstd 解码）

父任务：`.trellis/tasks/08-16-passive-sources-zcode-antigravity-deepseek`（调研见父任务 `research/deepseek-harness.md`，含 2026-08-16 二次校验修订）。

> **范围修订（2026-08-16 二次校验后）**：初版与审阅报告都基于"本机 `~/.deepseek/sessions` 为空 → 无样本 → 停止规则"把本任务定为仅登记。二次核查发现 DSH 真根是 `~/.dsh/`，本机有 **15 个真实会话（5 空 + 10 正常）**，usage 记录、官方 token 语义、文件布局全部齐备——停止规则解除，任务升级为复杂任务（登记 + 解析器两阶段）。审阅中仍然成立的部分（不加 parser=false 的 SourceKind、platform_id 用 `deepseek_harness`、避免 `deepseek` 一词与 provider/模型名混淆）已吸收。

## Goal

为 DeepSeek Harness 建立 llmusage 被动数据源：先登记 platform monitor（`deepseek_harness`，Planned），再实现只读解析 `~/.dsh/sessions/--<cwd>--/<id>/session.jsonl.zstd`（多帧 zstd JSONL）的解析器，把 `assistant/message.data.usage` 导入为 `precise` 质量（total 为通道求和）的 `UsageEvent`。

## Requirements

### R1 依赖决策与证据闭合（第一道 gate，不通过则降级为仅登记）

- zstd 解码依赖评审：**首选 `zstd` crate**（跟随 tokscale 参考实现 `sessions/dsh.rs`——流式 `Decoder` 支持撕裂尾帧前缀恢复，其 Windows CI 已验证可构建）；备选纯 Rust `ruzstd`（CI 契约拒绝 C 依赖时，需先验证流式撕裂帧行为）。**验证必须真实变更 manifest**：`cargo add zstd` + `cargo build` + `cargo clippy`（`--dry-run` 不改 manifest，后续构建仍是旧依赖图，不算验证）；依赖更新后按 `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md` §6 跑 **MSRV 证明（隔离 `CARGO_TARGET_DIR`）** 与 `python scripts/ci-rust.py`。
- 多帧语义：独立 zstd 帧拼接，帧 1 = 恰一条 session 记录；**按帧魔数分派压缩与否**（不按扩展名，`compression: none` 写同名 `session.jsonl`）；尾帧截断保留已解前缀（durable boundary，DSH 自带 reader 同款）。
- **interrupted 样本缺口（onboarding 证据项）**：本机 15 个文件扫描**无一带撕裂尾帧**，正常/空两类齐备但"interrupted/error"类缺失（onboarding 要求三类真实样本）。补齐方式：真实运行中中断一次 dsh 会话（如写入中途 kill）采集真实撕裂帧文件，样本路径与脱敏说明写回 research；在此项闭合前解析器阶段不开工。

### R2 登记（阶段一，独立可交付）

- **不加 parser=false 的 SourceKind**（`source_status.rs:174-179`：无 parser 的 SourceKind 显示 `historical_only`，语义错误；blocked 只存在于 monitor）。
- platform monitor：`platform_id = "deepseek_harness"`、`source_kind = None`（gemini/reasonix 同款）、roots 探测 `~/.dsh`（主）与 `~/.deepseek`（旧根，仅探测报告）、artifact patterns `sessions/**/session.jsonl.zstd` 与 `session.jsonl`、`parser_status = Planned`、next_action 注明依赖评审与中断样本。
- `docs/agents/passive-source-candidates.md` 新增行：工件族、token 语义（官方 + 实测）、样本状态（15 个本地样本，interrupted 缺）、cursor 思路、Decision = Approved pending zstd dependency review（解析器落地后改 Approved as parser-backed）。

### R3 解析器（阶段二，与 SourceKind 同 PR）

- `SourceKind::DeepseekHarness`（stable id `deepseek_harness`）与 `DeepseekHarnessParser` 同批注册——不存在 parser=false 的中间态。
- 发现（对齐 tokscale `dsh-session-log` 契约）：`$DSH_HOME`（默认 `~/.dsh`）/ `sessions/` 下**任意深度**、文件名精确等于 `session.jsonl(.zstd)`。
- 解析：只读 `assistant/message` 的 `data.usage`（`assistant/chunk` 双带同值不读）；**模型归属优先 `data.message.source.{provider, model}`**（本机 835/835 全有），`request/header` 就近兜底；会话头 `id/createdAt/cwd/seedLength`（cwd 哈希；头缺失用文件父目录名作 session id）。
- **fork 双计防线**（tokscale 官方快照证实）：① 会话头带 `seedLength` 时跳过 `seq < seedLength` 的行（fork 复制的父前缀）；② **event_key 恒为复合键**——`message.id（或 sid 兜底）+ time + provider + model + 全部 token 通道`一起进键（fork 拷贝行与父行全字段一致 → 键相同跨文件折叠；非空但重复的脱敏占位 id 靠其余字段分离不同调用）。
- **跨文件所有权（会话家族重放）**：`event_key` 是全局主键而 reset 按单文件 path 删除——盘点期建立 `session→file` 与 `parentSession→child files` 映射，文件变化需 reset 时**强制重放同家族成员**（即使其 fingerprint 未变），防"owner 重写移除共享事件、未变化副本被 cursor 跳过导致事件消失"。
- 归一化（官方语义 + 本机实测）：`input = inputTokens`（**不含 cache**）、`cache_read = cacheReadTokens`、`cache_creation = cacheWriteTokens`（缺省 0）、`output = outputTokens`（**含 reasoning 原样保留**）、`reasoning = reasoningTokens`（诊断，不计 total）、`total = input + cache_read + cache_creation + outputTokens`（= DSH 官方 meter 口径）。
- **流式与资源上限**：文件 → 流式 Decoder → 行分割管道，不整体物化解压结果；行分割接入 4 MiB 单记录上限（`DEFAULT_MAX_JSONL_RECORD_BYTES`）与 partial-tail 语义。
- 跳过：usage 全零、`time` 缺失/非正、`seq < seedLength`；空会话（5 条状态记录）干净跳过；会话头 `version != 0` 照常解析但 parse issue 计数观测漂移。

### R4 增量与幂等

- per-file `FileCursor`（全量模式：fingerprint 变化 → 家族重放 + event_key 幂等）。
- **bounded run（`--recent-days`）**：按记录 `time` 过滤，**不推进 cursor、不执行整文件 reset/家族重放**；随后全量 sync 必须能恢复窗口外历史（契约 `source-sync-contracts.md:86-88`）。
- 会话文件删除：source_file 三态机 missing 保护。

### R5 测试（onboarding gate 全项）

- fixture：合成 `session.jsonl`（未压缩拼写）+ 多帧压缩 `.zstd`（含**截断尾帧**）；空会话 fixture；**fork 父子双文件**（带 seedLength 与不带两种）；占位符 message.id 行；**超 4 MiB 单记录**行。
- 集成：sync-twice 幂等、append 新帧只导新事件、rewrite/reparse 替换旧行、删除保历史、missing root `passive_no_data`（monitor 探测旧根 `~/.deepseek`）、`DSH_HOME` 覆盖、token 归一化断言（实测 7619/19840/171；官方快照 2885/25/23 → total 2910）、fork 不双计、**所有权重放**（owner 重写移除共享事件 + duplicate 未变化 → 事件不消失）、**bounded run 不推 cursor 不 reset**。
- 注册表不变量、source-status 状态断言。

### R6 文档

- `docs/agents/passive-source-candidates.md` Decision 更新；`README.md` / `README.zh-CN.md` / docs 页；ADR（新来源 + zstd 依赖决策）。

## Acceptance Criteria

- [ ] 阶段一：monitor 登记 + 候选表行合入，sync 不写 usage 行，`source-status`（无 `--source` 参数，核对输出中 deepseek_harness 条目）报 Planned 态与根探测。
- [ ] 阶段二：本机 sync 导入 15 个会话的 usage；抽查 2 个会话与手工解码对账一致（记入 research）。
- [ ] `llmusage source-status` 输出中 deepseek_harness 行为 `passive_ready`（有数据）/ `passive_no_data`（无数据），quality `precise`。
- [ ] 依赖验证真实落盘：manifest 变更 + `python scripts/ci-rust.py` + 隔离 target 的 MSRV 证明通过（非 `--dry-run`）。
- [ ] R5 测试全过（含所有权重放、bounded run、撕裂帧、超大记录）；`cargo test --all-features -- --test-threads=1`、`just ci` 全绿。
- [ ] interrupted 真实样本采集并写入 research（脱敏说明 + 路径）。
- [ ] 无 prompt/工具结果文本落库（只读 usage/model/时间戳/哈希化 cwd）；fixture 全合成脱敏。

## 非目标

- SQLite 会话后端（官方提及、本机无痕迹，未证实——记 research，不实现）。
- 帧边界增量解压优化（首版全文件重解析）。
- deepseek 模型计价目录条目（首版 Unpriced）。
