# PRD：zcode 被动解析器（SQLite model_usage）

父任务：`.trellis/tasks/08-16-passive-sources-zcode-antigravity-deepseek`（证据见父任务 `research/zcode-artifacts.md`、`research/ccusage-tokscale-reference.md`）。

## Goal

为 llmusage 新增 `zcode` 被动数据源：只读解析 `~/.zcode/cli/db/db.sqlite` 的 `model_usage` 表，把 zcode（Z.ai CLI）每次模型调用的 token 用量导入为 `precise` 质量的 `UsageEvent`，支持增量同步、幂等重跑与标准被动解析器测试面。

## Problem / 背景

- 用户要求统计 zcode 用量；llmusage 目前无此来源。
- tokscale 已有 zcode 支持（`ref/repo/tokscale/crates/tokscale-core/src/sessions/zcode.rs`），但其 JSONL 主路径 `~/.zcode/projects/` 在当前 zcode 版本已不落盘（本机为空）；现行真源是 `cli/db/db.sqlite`（本机 1030 行）与 `cli/rollout/*.jsonl`。
- token 语义已验证：`inputTokens` 含 cache（`total == in + out` 且 `cacheRead <= input`，20/20 样本成立）。

## Requirements

### R1 来源注册

- `SourceKind::Zcode`（stable id `zcode`，serde snake_case，clap value `zcode`）。
- `SOURCE_DESCRIPTORS`：`capabilities.parser = true`、`passive_probe` 对齐同类、`quality = Precise`、`privacy = LocalDatabase`。
- `PLATFORM_MONITORS`：roots 指向 `~/.zcode`（env 覆盖 `ZCODE_HOME`），artifact pattern `cli/db/db.sqlite`，`parser_status = Registered`，next_action 描述解锁后状态。
- `registry::registered_parsers()` 注册新解析器；`expected_token_accounting_version` 对齐其他来源（=2）。

### R2 发现与只读访问

- DB 定位：`$ZCODE_HOME/cli/db/db.sqlite`，默认 `~/.zcode/cli/db/db.sqlite`；env 语义对齐 `KIMI_CODE_HOME`（测试用）。
- 只读 URI 打开（`mode=ro`），不触碰 `-wal/-shm`；根缺失/DB 缺失 → 该源 `passive_no_data`，sync 整体不失败。
- schema 防御：探测 `computed_total_tokens` 列；缺失时降级 `provider_total_tokens`，再缺失通道求和。

### R3 解析与 token 归一化

- 仅导入 `status = 'completed'` 行；`error`/`cancelled` 跳过并计入 parse issues 计数（不落样本文本）。
- 通道映射（cache-inclusive 修正）：
  - `input = input_tokens - cache_read_input_tokens`
  - `cache_read = cache_read_input_tokens`
  - `cache_creation = cache_creation_input_tokens`
  - `output = output_tokens`
  - `reasoning = reasoning_tokens`
  - `total`：以 `computed_total_tokens` 校验（与 `input_tokens + output_tokens` 偏差时保留通道值并记录 issue，不静默改数）。
- model：`model_id`（如 `GLM-5.3`）经现有 `normalize_model`；`variant`/`agent` 可并入 model 元数据或忽略（设计定夺）。
- 时间戳：`started_at`（epoch 毫秒 → RFC3339）。

### R4 增量游标

- 复用 opencode 高水位模式：cursor 记 `last_completed_at + last_processed_ids`（锚点集合），分页读取、按页提交、锚点缺失自动重置重放。**水位锚定 `completed_at` 而非 `started_at`**（只有 completed 行可见，杜绝"晚完成请求因 started_at 落在水位下被永久漏掉"）；事件时间戳仍用 `started_at`（报告语义）。
- 事件查询：status 过滤在最外层、范围条件加括号；`computed_total_tokens` 列缺失时用 `CAST(NULL AS INTEGER)` 变体投影 + 代码降级（不能"始终 SELECT"否则旧 schema 查询即失败）；error/cancelled 计数走独立聚合查询。
- bounded run（`--recent-days`）：以 `completed_at >= cutoff` 过滤，复用已存水位为下界但**不推进水位/锚点**、不 reset（契约 `source-sync-contracts.md:86-88`）。
- event_key：`zcode:<hash(id)>`（id 唯一，主键）。

### R5 测试（onboarding gate 全项）

- fixture 单测：合成 SQLite（含 completed/error/cancelled 行、缺失 computed_total_tokens 的旧 schema 变体）→ `UsageEvent` 断言。
- sync-twice 幂等集成测试；追加行只导入新行；**晚完成请求不被水位漏掉**（started 早、completed 晚的行）；error/cancelled 跳过且计数正确；DB 重建/删锚点行的 cursor 回归；bounded run 不推水位且窗口外可被全量恢复；`--rebuild` 与 source_file guard；`passive_no_data → passive_ready` 状态测试；`ZCODE_HOME` 覆盖测试。
- 注册表不变量测试同步更新（descriptor/monitor/stable id 唯一性）。

### R6 计价与文档

- 首版 `Unpriced`（`static-v2.json` 无 zhipu/GLM 条目），与 grok/kimi/pi 一致；不在本任务加价目。
- 更新 `docs/agents/passive-source-candidates.md`（新增 zcode 行，Decision=Approved as parser-backed `zcode`）、`README.md`、`README.zh-CN.md` 与相关 docs 页。

## Acceptance Criteria

- [ ] `cargo run -- sync` 在有 zcode 数据的机器上导入 `model_usage` completed 行；`llmusage source-status`（当前 CLI 无 `--source` 参数，核对输出中 zcode 行）报 `passive_ready` + `precise`。
- [ ] 二次 sync 零新增；DB 追加新行后只导入增量。
- [ ] 全部 R5 测试通过；`cargo test --all-features -- --test-threads=1` 与 `just ci` 全绿。
- [ ] 无 prompt/对话文本进入数据库或日志（只读 `model_usage`，不读 `message`/`part`/`input_history`）。
- [ ] 未安装 zcode 的环境 sync 正常（absent/no_data，不报错）。
- [ ] 文档与候选表更新完成。

## 非目标

- 不解析 `~/.zcode/projects/`（旧布局已废弃）与 `cli/rollout/*.jsonl`（避免双源重复计数；rollout 作为研究证据保留）。
- 不实现 GLM 计价目录条目。
- 不读取 IDE 侧 `~/.zcode/v2/` 任务索引。
