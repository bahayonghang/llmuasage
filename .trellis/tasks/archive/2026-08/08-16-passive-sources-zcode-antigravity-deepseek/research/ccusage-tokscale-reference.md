# ccusage / tokscale 参考实现分析（ref/repo/）

调研日期：2026-08-16（**ref 更新后二次核对**：tokscale 快照已含 DeepSeek Harness 支持 `59712ada` (#1126)、antigravity 修复 `416721e2`/`2b45f576`、zcode 只读打开重构 `d8b23d14` (#1093)、ccusage import 格式 `25df658c`）。来源：`ref/repo/ccusage`（Rust workspace + npm 启动器）、`ref/repo/tokscale`（Rust workspace + bun/npm 包装）。`ref/repo/agentsview` 为桌面查看器，与被动解析无关。

## 1. ccusage

- 架构：`rust/adapters/<agent>/` 每来源一个 crate，固定形状 `paths.rs`（env var + 目录发现）、`parser.rs`（记录→token 映射）、`loader.rs`（遍历、SQLite、去重、日期过滤）、`report.rs`、`types.rs`；共享逻辑在 `rust/adapters/common/` 与 `rust/crates/ccusage-core/`。
- 16 个适配器：`amp, claude, codebuff, codex, copilot, droid, gemini, goose, grok, hermes, kilo, kimi, openclaw, opencode, pi, qwen`。**没有 zcode、antigravity、deepseek 适配器**（deepseek 仅作为 pi 适配器测试里的计价模型名出现）。
- Claude 发现规则（`rust/adapters/claude/src/paths.rs`）：`CLAUDE_CONFIG_DIR`（逗号分隔，归一到 `<dir>/projects`）→ `~/.config/claude` → `~/.claude`，遍历 `projects/**/*.jsonl`。
- 性能技巧：`memmem::Finder::new(br#""usage":{"#)` 字节级预过滤，无 usage 标记的行跳过 JSON 解析；`has_unsupported_null_field()` 拒绝关键字段为 null 的行。
- Token 字段：`TokenUsageRaw { input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens, speed, cache_creation{ephemeral_5m, ephemeral_1h} }`；成本优先用行内 `costUSD`，否则按 `PricingMap`（LiteLLM JSON + models.dev 兜底）计算。
- 去重：`usage_dedupe_hash(message_id, request_id)`；sidechain 重放按 message-id 匹配，候选非 sidechain 或 token 总量更大时替换。
- Pi/OMP（`rust/adapters/pi/src/`）：`~/.pi/agent/sessions` + `~/.omp/agent/sessions`（named_store_paths），OMP 模型加 `[omp] ` 前缀后查价。

## 2. tokscale

- 声明式客户端注册表（`crates/tokscale-core/src/clients.rs`）：`define_clients!` 宏表，45 个 `ClientId`，每项 `{id, display, root: PathRoot, relative, pattern, headless, parse_local, submit_default}`；`PathRoot::{Home, XdgData, Config, EnvVar{var,fallback}}`。`resolve_path_with_env_strategy` 只在 `use_env_roots=true` 时尊重 env 覆盖（显式 `--home` 会禁用 env）。
- 与本任务相关的路径（`clients.rs` + `scanner.rs`）：
  - **zcode**：`~/.zcode/projects/**/*.jsonl`（JSONL，注册表）+ `~/.zcode/cli/db/db.sqlite`（scanner 直连 v2 CLI SQLite）。
  - **omp**：`~/.omp/agent/sessions`，挂在 `ClientId::Pi` 下（注释：“Oh My Pi fork … same JSONL format, different root”）。
  - **antigravity（IDE）**：`~/.config/tokscale/antigravity-cache/sessions/*.jsonl`——这是 tokscale 自己 RPC 钩子落盘的缓存，**不是**原生工件，不可复制。
  - **antigravity-cli**：`$GEMINI_CLI_HOME | ~/.gemini/antigravity-cli/conversations/*.db`（SQLite）。
  - **kimi**：`~/.kimi/sessions/**/wire.jsonl`（旧 CLI）与 `~/.kimi-code/sessions/WORKSPACE/SESSION/agents/AGENT/wire.jsonl`（Kimi Code，`KIMI_CODE_HOME`）。
  - **grok**：`$GROK_HOME | ~/.grok/sessions/**/updates.jsonl` + `~/.grok/logs/unified.jsonl`（双源）。
  - **deepseek / DSH**：**ref 更新后已有**（`59712ada` #1126）——`ClientId::Dsh = 46`（`id: "dsh"`，`PathRoot::EnvVar { var: "DSH_HOME", fallback_relative: ".dsh" }`，relative `sessions`，pattern `dsh-session-log`：文件名精确 `session.jsonl.zstd` 或 `session.jsonl`、sessions/ 下任意深度），解析器 `sessions/dsh.rs`（zstd crate 流式解码、撕裂尾帧前缀恢复、seedLength fork 守卫、message.id 去重键）。详细分析见 `deepseek-harness.md` §5.1。
- 统一模型：`UnifiedMessage { client, model_id, provider_id, session_id, workspace_key/label, timestamp, tokens: TokenBreakdown{input, output, cache_read, cache_write, reasoning}, cost, cost_source, dedup_key, ... }`；`CostSource::{Unknown, ProviderReported, Estimated}` 保证重定价不覆盖权威成本。
- 增量状态（`crates/tokscale-core/src/message_cache.rs`）：持久 bincode 缓存 `<config>/cache/source-message-cache-v2/`（256 分片 + 锁文件，`CACHE_FORMAT_VERSION=5`，每客户端 `parser_version()`）；`SourceFingerprint { size, modified_ns, 5×4KiB sample_hashes, content_hash }`。Claude 通道用“保留历史”变体处理 resume/compact 造成的原地重写。
- 计价：custom-pricing.json → LiteLLM → OpenRouter → models.dev 并发拉取、各自独立降级到本地过期缓存；128k/200k/256k/272k 分档；reasoning 计入 output。
- SQLite 访问：`open_readonly_sqlite_opt` 只读打开；zcode 用“探测列存在”（`computed_total_tokens`）而非 query 失败来区分新旧 schema。

## 3. 与 zcode/antigravity 直接相关的参考文件

- `crates/tokscale-core/src/sessions/zcode.rs`
  - `parse_zcode_file`：读 `~/.zcode/projects/<slug>/<session>.jsonl`，字段别名 `input_tokens|prompt_tokens|inputTokens`、`output_tokens|completion_tokens|outputTokens`、`input_cache_read|cache_read_tokens|cacheReadTokens`、`input_cache_creation|cache_write_tokens|cacheCreationTokens`、`reasoningTokens`、`totalTokens`；无 usage 时按 `chars.div_ceil(4)` 估算。
  - `parse_zcode_sqlite`：`model_usage` join `session`；新 schema 有 `computed_total_tokens` 列，旧 schema 无条件减重叠。
  - `normalize_zcode_input_and_output`（核心语义）：ZCode 上报的 `input_tokens` 含 cache、`output_tokens` 含 reasoning，用上报 `total` 检测形状并减去重叠。
  - dedup：`zcode-sqlite:{row_id}`；provider `"zhipu"`；默认模型 `"glm-5.2"`。
  - 注意：tokscale 注释明确该 ZCode 是 Z.ai 的 ADE；`~/.zcode/projects` 布局在当前版本已不落盘（本机该目录为空），`cli/db/db.sqlite` 才是现行真源。
- `crates/tokscale-core/src/sessions/antigravity_cli.rs`
  - 读 `conversations/*.db` 的 `gen_metadata` 表，每行是一个 `GeneratorMetadata` protobuf，手写 wire-format 读取器：字段 `#4` 为 usage `{#1 fixed system prompt, #2 non-cached input, #5 cacheRead, #9 output, #10 thinking, #11 responseId}`，`#19` responseModel，`#21` display label。
  - `SessionModels` 从兄弟行恢复缺失的 model id。
- `crates/tokscale-core/src/sessions/antigravity.rs`：IDE 侧读 tokscale 自建 RPC 缓存（`session_meta`/`usage` 行，占位模型别名 `MODEL_PLACEHOLDER_M26` → `claude-opus-4-6` 等）——**不可用于 llmusage 被动解析**，但占位模型映射思路可借鉴。
- `crates/tokscale-core/src/sessions/grok.rs` / ccusage `rust/adapters/grok/src/paths.rs`：与 llmusage 现有 grok 解析器同源思路（updates.jsonl turn delta）。

## 4. 值得借鉴的模式（llmusage 对应物）

| tokscale/ccusage 模式 | llmusage 已有等价物 |
| --- | --- |
| 每来源 adapter crate（paths/parser/loader 分离） | `src/parsers/<source>.rs` + `src/parsers/source_files.rs` 发现层 |
| 声明式 client 注册表 | `src/domain/source_descriptor.rs::SOURCE_DESCRIPTORS` + `src/domain/platform_monitor.rs::PLATFORM_MONITORS` |
| UnifiedMessage + 5 桶 token | `UsageEvent`/`UsageTokens` + `UsageQuality` |
| CostSource 防覆盖 | `PricingStatus::{Priced, Unpriced}` |
| SourceFingerprint 缓存 | `FileCursor`（fingerprint+offset）+ `source_file` 三态机 |
| dedup_key | `event_key`（`pi:<hash>` 等，按 path_hash+offset 构造） |
| 只读 SQLite + schema 探测 | opencode 解析器只读连接（llmusage 已有 rusqlite 只读路径） |
| cache-inclusive 归一化（total 检测） | zcode 子任务需要新增同等逻辑（本机证据见 zcode-artifacts.md） |
