# 技术设计：Grok Build 被动源（grok）

前置阅读：`research/grok-build-evidence.md`（数据形态与坑）、`.trellis/spec/llmusage/backend/source-sync-contracts.md`、`token-accounting-contracts.md`、`write-fencing-contracts.md`。

> 2026-07-27 修订：吸收外部审核结论——① MVP 保持 unpriced；② 幂等改为"会话原子重放"（复用现有 reset 协议）；③ 放弃 offset 续读，消除轮序号键碰撞；④ 扫描改固定两级枚举；⑤ 真机验证用 `--home` 隔离。

## 总体形状

完全复用 `pi` / `kimi_code` 的被动源模式（参考提交 `4d6b04e` 的改动面）：一个 `SourceKind` 变体 + 一个 parser 模块 + descriptor/monitor 注册 + sync/status/TUI/web 呈现 + 文档。不新增抽象层。

## 数据流

```
GROK_HOME（回退 ~/.grok）
  └─ sessions/<urlencoded-workspace>/<session-id>/     ← 固定两级枚举
       ├─ updates.jsonl   → 逐轮增量事件（若含 totalTokens 计数器）
       ├─ signals.json    → 会话级对账事件（差额）
       ├─ summary.json    → 元数据（model、时间、cwd）
       └─ events.jsonl    → 元数据兜底（可选）
                ↓ 会话级全量 parse → Vec<UsageEvent>（子通道 0，total 权威）
        sidecar 变化 → reset_path_hashes + 重插（原子重放）
                ↓
        store → query → CLI/TUI/web（pricing_status = unpriced）
```

## 模块边界与触点（对照 4d6b04e）

| 触点 | 改动 |
| --- | --- |
| `src/domain/models.rs` | 新增 `SourceKind::Grok`，id 字符串 `grok` |
| `src/registry.rs` | `parse_source_id("grok")` 与源清单注册 |
| `src/domain/source_descriptor.rs` | 被动 descriptor：root 解析（`GROK_HOME` env → `~/.grok`）、`quality: UsageQuality::TotalOnly`、`privacy: LocalArtifacts` |
| `src/domain/platform_monitor.rs` | probe：目录存在性 → `passive_ready` / `passive_no_data` |
| `src/parsers/grok.rs`（新建） | 解析核心，见下节 |
| `src/parsers/mod.rs` | 注册 `(SourceKind::Grok, grok::bounded_contract_parse)` |
| `src/parsers/source_files.rs` | **新增 grok 专用枚举函数**：固定两级目录遍历 + 白名单文件名 join（不走通用 `list_matching_files`/WalkDir 递归） |
| `src/store/schema.rs` | `SourceKind::Grok` 加入 `TOKEN_ACCOUNTING_VERSION` 分支 |
| `src/commands/{sync,sync_progress,sync_summary,doctor,diagnostics,help}.rs` | 汇总/状态/帮助文案 |
| `src/tui/report_table.rs` | 分配来源色 |
| `src/query/mod.rs`、`src/web/mod.rs`、`src/api/error.rs` | 来源枚举透传 |
| `tests/sync_regression.rs`（或新建 `tests/grok_flow.rs`） | 集成测试 |
| 文档 | README 中英、`docs/{architecture,guide/first-sync,reference/cli,dashboard,index}` 及 `docs/zh/*` 对应页、`docs/agents/passive-source-candidates.md` |

**不改动**：pricing catalog（MVP 无 grok 价格行，见下文"定价"）。

## 会话枚举（source_files.rs）

通用 `list_matching_files` 用 `WalkDir` 全递归后按文件名过滤（`source_files.rs:132`），会进入 `terminal/`（本机实测含阻塞读取的特殊文件）。grok 走专用枚举：

```
for workspace_dir in read_dir(root/"sessions")        // 第一级
  for session_dir in read_dir(workspace_dir)          // 第二级
    直接 join: updates.jsonl / signals.json / summary.json / events.jsonl
```

只 stat/open 白名单路径，`terminal/`、`*.lock`、`chat_history.jsonl` 等永不触达。安全证明分两层：实现审查确认 Grok 枚举函数只有两层 `read_dir`、不调用 `WalkDir`；fixture 测试用 `terminal/` 嵌套哨兵断言候选集只有会话根白名单文件且无嵌套枚举错误。结果集合测试本身不宣称能证明底层从未访问，避免不可证伪断言。

## 解析器设计（src/parsers/grok.rs）

### 幂等模型：会话 = 原子重放单元

- 会话全部事件共享 `source_path_hash = hash(会话目录相对标识)`（对齐现有 path_hash 生成方式）。
- `FileCursor` 每个 sidecar 一条，仅作**变化检测**（fingerprint/size/mtime），不做 offset 续读。
- 任一 sidecar 变化 → 该会话完整重parse → shard 输出 `reset_path_hashes=[session_hash]` + 全量事件。sync writer 现有 `reset_file_events_batch_tx`（`sync_writer.rs:112`，被 `:563` 在事务内调用）负责删除旧事件并回退聚合桶，与 claude/codex/kimi_code 的 reset 协议一致。
- 无变化 → 跳过，零写入。
- 枚举器以**会话目录**为候选单元，同时返回当前白名单 sidecar 集；parser 把 cursor/source-file inventory 中该会话的历史 sidecar 与当前集合比较。已追踪 sidecar 缺失时，常规 sync 只记录 missing 诊断，不提交 `reset_path_hashes`、event 或 cursor 覆盖，避免用不完整会话抹掉历史。显式 rebuild 继续走现有 missing-file / lossy-rebuild guard；sidecar 恢复后再完整重放。

该模型一次性解决三个审核发现：signals.json 原地增长的差额补插不再撞 `INSERT OR IGNORE` 唯一键（每次重放整体重建）；updates 后续补齐逐轮数据时 signals 对账额自然被重算回收；turn_index 永远从全文件解析得出，无续读碰撞。代价是 sidecar 每变化一次就重parse 整个会话文件（本机最大 3.7MB，可接受）。

### 路径 A：updates.jsonl 逐轮增量

- 逐行 JSON；从候选路径提取累计计数器：`params._meta.totalTokens`、`params.update._meta.totalTokens`、`params.update.totalTokens`、`params.totalTokens`、`usage.totalTokens`、`totalTokens`。**不依赖 method 名**（本机为 `_x.ai/session/update`，tokscale 样本为 `session/update`）。
- `params.update.sessionUpdate == "user_message_chunk"` 开新轮；轮事件 token = 轮内最大计数 − 基线；≤0 丢弃。
- 计数器单调化：小于前值的行跳过（流式工具更新会回退）。
- 无任何 user_message_chunk 但有计数增长时，产出单条聚合事件（tokscale 的 fallback）。
- 时间戳候选：`params._meta.agentTimestampMs` → `params.update._meta.agentTimestampMs` → `params.timestamp` → 顶层 `timestamp`/`ts`；`*TimestampMs` 按 Unix 毫秒，10 位左右数值按 Unix 秒，13 位左右数值按 Unix 毫秒，字符串只接受 RFC3339 或可无歧义解析的十进制秒/毫秒。无效或缺失回退 summary.json 的 `updated_at`。
- 事件键：`grok:<session_id>:<turn_index>`。在全量重放模型下 turn_index 稳定（总是从文件头解析）。

### 路径 B：signals.json 会话对账

- `effective_total = max(totalTokens, totalTokensBeforeCompaction + contextTokensUsed)`（字段缺失按 0，负数钳 0）。
- 对账差额 = `effective_total − 路径A 事件总和`；>0 时产出一条事件，事件键 `grok:<session_id>:signals`。同会话重放时整体删除重建，键固定不产生冲突。
- 时间戳：锚定路径 A 最后事件时间；无路径 A 数据时回退 summary.json `updated_at`（**绝不用 signals.json mtime**，理由见 research）。

### 元数据

- session_id：目录名；workspace：父目录名 percent-decode（自实现 ~30 行 decoder，参考 tokscale，不新增依赖）后走现有 workspace 归一化。
- 模型回退链：updates `_meta.modelId` → signals `primaryModelId`/`modelsUsed[0]` → summary `current_model_id` → `grok-unknown`。
- `provider_label`：**保持空串**。该字段是 CCR 中继归属（`models.rs:176`），不承载 provider 身份；`xai` 不落库。
- token 通道：`total_tokens` 写权威总量，input/cache_read/cache_creation/output/reasoning 全 0。实现前对照 `token-accounting-contracts.md` 确认 total 权威字段在子通道全 0 时的既有语义（bucket 聚合、报表 total 列均直接可用）；如契约无此先例，先补契约再写码。

## 定价（MVP：unpriced）

**不添加 grok 定价行。** 现有 `compute_cost_with`（`src/query/pricing.rs`）只消费 `CostTokens` 的五个子通道、不读 total；grok 事件子通道全 0，若价格行存在会得到 `pricing_status=matched` 且成本恒 $0 的错误精确性。无价格行时 `catalog.find` 落空 → `PricingStatus::Unpriced`，成本列如实显示未估算。文档写明原因与未来路径（若后续版本 Grok 落盘 input/output 拆分，或决定引入明确标注的 `Estimated` 全额按 input 计价，再开任务）。

## 权衡与风险

- **本机 0.2.112 的覆盖缺口**：8 个会话仅 1 个有 signals.json，7 个合法产出 0 事件。接受此缺口并在 source-status 文案/文档中说明"Grok Build 仅部分会话落盘 token 汇总"；不做估算兜底。
- **updates.jsonl 路径在本机拿不到验证数据**：单元测试用 tokscale 移植的 fixture 保障；真实端到端验证只覆盖 signals 路径。风险登记在 implement.md 验证步骤。
- **重放放大**：活跃会话每次 sync 都会整会话重parse+重插。量级（单文件 ≤ 数 MB、会话数十）下可接受；与 claude 源的 reset 行为同型。
- **回滚**：纯增量接入。回滚 = 移除 `SourceKind::Grok` 注册；真机验证使用 `--home` 临时目录，默认用户库全程不被触碰。

## 兼容性

- 不改既有事件 schema 行；`TOKEN_ACCOUNTING_VERSION` 分支新增枚举臂不触发老数据迁移（与 kimi_code/pi 加入时相同）。
- Windows/Unix 路径：workspace decode 后是原生路径字符串，交给现有 workspace 归一化处理，不假设分隔符。
- 实施基线：`dev` 分支（task.json base_branch 已由 main 修正为 dev）。
