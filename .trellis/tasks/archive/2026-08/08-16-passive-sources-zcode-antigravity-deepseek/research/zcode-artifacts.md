# zcode 本地产物证据与 token 语义

调研日期：2026-08-16，样本来自本机 `C:\Users\lyh\.zcode\`（zcode CLI 正在使用中）。初稿后经二次校验修订（2026-08-16 晚）：列数 42→40，补充 deepseek-v4-flash 模型证据与 output/reasoning 形状结论（初稿只看了 GLM 行，结论不完整）。

## 0. 二次校验摘要（权威数据，2026-08-16 晚实测 1070 行）

- `model_usage` 共 **40 列**（初稿误写 42；列名清单本身正确）。
- 模型分布：`deepseek-v4-flash` 665 行、`GLM-5.3` 405 行——**过半是 deepseek**，token 归一化不能只按 GLM 形状设计。
- `reasoning_tokens > 0`：473 行，**全部是 deepseek-v4-flash**；GLM 全 0。
- `computed_total_tokens == input_tokens + output_tokens`：completed 行全表成立（0 违反）；`total == in + out + reasoning` 仅在 reasoning=0 的行成立 → **total 从不单独加 reasoning，output 含 reasoning**。
- 例证行 `(input=316, cache_read=256, output=391, reasoning=380, total=707)`：316+391=707 ✓，reasoning 380 ≤ output 391 → reasoning 是 output 的子集。
- `status != 'completed'`（error 13 + cancelled 2）的行 token 全为 0 → 过滤 completed 不丢用量。
- `id` 唯一（0 重复），但 **1067+/1070 行 id ≠ `logical_request_id`**、也 ≠ `logical_request_id || '_' || attempt_index`（实际形如 `usage_model_main_turn_msg_<msgkey>…`、`usage_model_session_title_<uuid>…`）→ event_key 只哈希 `id` 即可，初稿"已含 logical_request_id 语义"的说法作废。
- `session` 表列（实测）：`id, project_id, workspace_id, parent_id, slug, directory, path, title, version, share_url, summary_*, revert, permission, time_created, time_updated, time_compacting, time_archived, task_type, title_source, title_message_id, time_title_updated, trace_id`。若做 project 归属只哈希 `directory`/`path`，**不落 `title`**。

## 1. 目录布局（本机实测）

```
~/.zcode/
├── cli/
│   ├── db/db.sqlite (+ -wal/-shm)     ← 现行权威真源
│   ├── rollout/model-io-sess_<uuid>.jsonl   ← 每次模型调用的请求/响应滚动日志
│   │   └── model-io-no-session.jsonl        ← 无会话归属的调用
│   ├── agents/ artifacts/ exec/ log/ plugins/
├── v2/
│   ├── tasks-index.sqlite (+ -wal/-shm)  ← 任务索引（automation/task 表，无 usage）
│   ├── config.json / credentials.json / logs/ / telemetry-state.json
├── projects/        ← 空目录（tokscale 旧版 JSONL 布局，当前版本不再写入）
├── workspace/ plugin-workspace/
```

要点：**tokscale 的 JSONL 主路径 `~/.zcode/projects/**/*.jsonl` 在本机为空**，实现不能只按旧布局走；现行版本把 usage 落在 `cli/db/db.sqlite` 与 `cli/rollout/*.jsonl`。

## 2. 真源一：`cli/db/db.sqlite` 的 `model_usage` 表（推荐主源）

列清单与行数以 §0 二次校验为准（**40 列**；行数持续增长，2026-08-16 晚为 1070）：

`id, logical_request_id, attempt_index, session_id, turn_id, trace_id, span_id, assistant_message_id, parent_user_message_id, query_source, provider_id, model_id, variant, agent, mode, task_type, status, started_at, first_token_at, completed_at, duration_ms, time_to_first_token_ms, finish_reason, tool_call_count, input_tokens, output_tokens, reasoning_tokens, cache_creation_input_tokens, cache_read_input_tokens, provider_total_tokens, computed_total_tokens, retry_count, retryable, cancelled_by_user, context_exceeded, error_type, error_code, error_message, raw_usage_json, provider_metadata_json`

本机数据特征：

- `status` 分布：`completed` 绝大多数、`error` 13、`cancelled` 2；非 completed 行 token 全 0（§0）。
- `raw_usage_json` 原样保存 provider 响应 usage，例：
  `{"inputTokens":60543,"outputTokens":3254,"totalTokens":63797,"cacheReadTokens":56960,"cacheWriteTokens":0}`
- `computed_total_tokens == provider_total_tokens == input_tokens + output_tokens`（GLM-5.3 样本一致）。
- `model_id` 如 `GLM-5.3`；`provider_id` 如 `builtin:bigmodel-start-plan`；`variant` 如 `max`；`started_at/completed_at` 为 INTEGER（epoch 毫秒）。
- 同库还有 `turn_usage`（每 turn 聚合，31 行）、`session`、`message`/`part`（完整对话文本，**不得读取**）、`tool_usage`、`schema_migration`（schema 有版本表）。

### token 语义（关键结论，二次校验后修订）

`rollout` JSONL 与 `raw_usage_json` 的 camelCase 字段一致：`{inputTokens, outputTokens, totalTokens, cacheReadTokens, cacheWriteTokens}`。

- **`inputTokens` 是 cache-inclusive（Anthropic 风格）**：`totalTokens == inputTokens + outputTokens` 全表成立；`totalTokens == in+out+cacheRead` 全部不成立；`cacheReadTokens <= inputTokens` 全部成立。
- **`outputTokens` 是 reasoning-inclusive**：deepseek-v4-flash 行 reasoning>0 且 reasoning ≤ output，total 仍 = in+out（473 行证据，见 §0）。
- `cache_creation_input_tokens` 列存在但本机全 0；归一化公式仍需预留（GLM 未来写 cache write 时 input 才不会虚高；codeburn 同款公式，外部参考未经本地验证）。
- 对照仓库契约 `.trellis/spec/llmusage/backend/token-accounting-contracts.md:29-34`：内部 `input_tokens` 通道必须是**非缓存 input**；reasoning 默认是诊断通道，**不加入 output 或 total**；可信上游 total 权威。因此：
  - `input` = `input_tokens - cache_read_input_tokens - cache_creation_input_tokens`（饱和减）
  - `output` = `output_tokens` 原样保留（含 reasoning，**不要**学 tokscale 再减 reasoning——减了会与权威 total `= in+out` 对不上）
  - `reasoning` = `reasoning_tokens`（诊断通道）
  - `total` = `computed_total_tokens` 权威值
- `status != 'completed'` 的行 token 全 0 → 只导入 `status='completed'` 行不丢用量（§0 实测）。

## 3. 真源二：`cli/rollout/model-io-*.jsonl`（辅助/备选）

每行一次模型调用（`type: "model_io"`），顶层键：
`attempt, completedAt, durationMs, model{modelId, providerId, role, source, variant}, querySource, request{body, headers, maxOutputTokens}, requestId, response{headers, modelId, usage{...}}, sessionId, startedAt, traceId, turnId, type`。

- `model.role` 观测值：`main` / `lite` / `subagent`（子代理会话独立文件 `model-io-sess_subagent_agent_<uuid>.jsonl`）。
- `response.usage` 与 `raw_usage_json` 同构（camelCase，cache-inclusive input）。
- **隐私**：`request.body` 含完整 prompt 消息体 → 解析器若读此源，只允许持久化 usage/model/时间戳，且 `bounded_contract_parse` 约定不得泄漏 body。
- 优点：append-only JSONL，天然契合现有 `BoundedJsonlReader` + `FileCursor`。
- 缺点：无 reasoning/cache_creation 通道；`attempt` 字段暗示重试行可能重复（同 `requestId` 多 attempt）。

## 4. 设计取舍：主源选 SQLite `model_usage`

| 维度 | SQLite `model_usage` | rollout JSONL |
| --- | --- | --- |
| token 通道 | 全（含 reasoning、cache_creation、computed_total） | 缺 reasoning/cache_creation |
| 请求级状态过滤 | 有（`status`/`finish_reason`/`error_*`） | 无（attempt 需自行去重） |
| 稳定主键 | `id` TEXT（logical request id）+ `attempt_index` | 需自己拼 requestId+attempt |
| 增量游标 | 高水位（`started_at` + id 集合），可复用 opencode 模式 | FileCursor（现有机制直接可用） |
| 写入活跃度 | WAL 活跃写（只读连接可行，opencode 同款） | append-only |

结论：**主源 = `~/.zcode/cli/db/db.sqlite::model_usage`**（status=completed），游标复用 opencode 的高水位分页提交模式（`src/parsers/opencode.rs`：`last_time_created` + `last_processed_ids` 锚点，锚点缺失自动重置）。rollout JSONL 仅作为研究证据记录，不实现双读（避免双源重复计数；tokscale 双读靠 dedup 兜底，llmusage 单源更简单且 `model_usage` 是超集）。

## 5. 发现与环境变量

- 根：`~/.zcode/cli/db/db.sqlite`；env 覆盖 `ZCODE_HOME`（对齐 `KIMI_CODE_HOME`/`GROK_HOME` 约定，指向 `~/.zcode` 等价目录，DB 路径为 `<root>/cli/db/db.sqlite`）。
- **env 命名说明（审阅修订）**：`ZCODE_HOME` 是 llmusage 自造覆盖名，zcode 官方无公开存储根变量；第三方工具用了别的名字（AgentPeek `ZCODE_STORAGE_DIR`、aiusage `ZCODE_DB`——外部参考，未本地验证）。design 必须写明：默认路径 `~/.zcode` 为准，`ZCODE_HOME` 仅是 llmusage 的测试/重定向覆盖；若发现 zcode 实际认其它官方变量，再加一层，不得只认自造名。
- 缺失根 → `passive_no_data`，sync 不报错（对齐 kimi/pi 行为）。
- `-wal/-shm` 文件不读；只读 URI 打开（`file:...?mode=ro`）。
- schema 防御：探测 `model_usage.computed_total_tokens` 列是否存在（tokscale 同款），缺失时降级 `provider_total_tokens`，再缺失则通道求和并标记 quality 退化（不阻塞导入）。

## 6. 隐私边界

- 读取：`model_usage`，外加 `session` 的 `directory`/`path` 列（project 归属，**哈希化，不落 `title`**——列清单见 §0）。不读 `message`/`part`/`input_history`。
- 持久化：归一化 usage 通道、model、session/turn id 的哈希、时间戳。`error_message`、`raw_usage_json` 之外的任何文本不落库。
- 采样脱敏：fixture 用 `raw_usage_json` 数值 + 假 uuid 构造，不含 prompt。

## 7. 定价

provider `zhipu`、模型 `GLM-5.3` 等不在 `pricing/static-v2.json` → 首版 `Unpriced`（与 grok/kimi/pi 一致）。后续可在 catalog 加 `sources.zcode` 条目（matcher 前缀 `glm-`），不在本任务阻塞范围。

## 8. 质量 label

`precise`（每请求四通道 + 权威 total，来自 provider 原始上报的 SQLite 真源）。
