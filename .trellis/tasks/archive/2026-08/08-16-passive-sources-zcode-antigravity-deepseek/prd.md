# PRD：新增 zcode / antigravity / deepseek-harness 被动数据源（父任务）

## Goal

按用户需求为 llmusage 补齐六个来源的数据统计能力。经调研，其中三个已经支持，本任务树只实现真正缺失的部分：**zcode 被动解析器**、**antigravity CLI 被动解析器（解除 blocked，含历史数据保护）**、**deepseek-harness 两阶段交付（monitor 登记 + zstd 解析器）**。

## 调研结论总表（2026-08-16，含二次校验修订）

| 用户列出的来源 | llmusage 现状 | 本地验证 | 结论 |
| --- | --- | --- | --- |
| omp（Oh My Pi） | `pi` 来源已覆盖 `~/.omp/agent/sessions/**/*.jsonl`（passive-source-candidates.md 第 24 行，Approved） | 本机存在 8 个 omp 会话 JSONL | **无需开发**（稳定 id 是 `pi`） |
| kimicode | `kimi_code` 解析器已批准（wire.jsonl，usageScope=turn） | 本机 `~/.kimi-code` 存在 | **无需开发**（稳定 id 是 `kimi_code`） |
| grok build | `grok` 解析器已批准（total_only/unpriced，ADR-0011） | 本机 `~/.grok` 存在 | **无需开发** |
| zcode | 无任何支持 | 本机 `~/.zcode/cli/db/db.sqlite::model_usage` 1070 行真实样本（deepseek-v4-flash 665 + GLM-5.3 405） | **子任务 1：完整解析器** |
| antigravity | `historical_only`（parser=false，blocked_no_samples，ADR-0011） | 本机 `~/.gemini/antigravity-cli/conversations/*.db` **81 个**真实样本，gen_metadata wire 结构已解码定稿 | **子任务 2：解除 blocked + 解析器** |
| deepseek harness | 无任何支持（ccusage/tokscale ref 快照亦无） | **真根是 `~/.dsh/`（非 `~/.deepseek/`）**：15 个真实会话样本（5 空 + 10 正常）含 usage 记录，官方 token 语义齐备 | **子任务 3：登记 + 解析器（两阶段）**——初判"无样本"系查错根，停止规则已解除 |

研究证据在父任务 `research/` 下：`ccusage-tokscale-reference.md`（ref 代码分析）、`zcode-artifacts.md`（含二次校验修订）、`antigravity-artifacts.md`（含 wire 嵌套修正）、`deepseek-harness.md`（含 `~/.dsh` 修正）。

## 任务地图

| 子任务 | 目录 | 复杂度 | 交付物 |
| --- | --- | --- | --- |
| zcode 被动解析器 | `08-16-zcode-passive-parser` | 复杂（prd+design+implement） | `SourceKind::Zcode` + SQLite model_usage 解析器 + 全套测试 |
| antigravity CLI 解析器 | `08-16-antigravity-cli-parser` | 复杂（prd+design+implement） | descriptor 翻转 + gen_metadata protobuf 解码解析器 + ADR 修订 + 全套测试 |
| deepseek-harness (dsh) | `08-16-deepseek-harness-descriptor` | 复杂（两阶段：登记 + 解析器） | platform monitor 登记 + `SourceKind::DeepseekHarness` + zstd 解析器 + 全套测试 |

依赖关系：三个子任务相互独立、可分别验证归档；无实现顺序依赖。建议按 1 → 2 → 3 顺序（zcode 无前置 gate 可直接开工；antigravity 需先补 R1 fixture 三项；deepseek 需先过 zstd 依赖评审）。

## 跨子任务验收标准（父任务持有）

- [ ] 三个子任务全部归档后，父任务执行最终集成复查（本清单）再归档：跨源 `source-status` 输出、六来源报表口径、README/docs 一致性。
- [ ] `cargo test --all-features -- --test-threads=1` 与 `just ci` 全绿（含 `src/registry.rs` 描述符/monitor 不变量测试、`tests/sync_regression.rs` 每源状态计数）。
- [ ] `llmusage source-status` 状态正确：**有数据的 parser 源（pi/kimi_code/grok/zcode/antigravity/deepseek_harness）报 `passive_ready`，无数据报 `passive_no_data`**（`source_status.rs:174-179` 的推导规则）；deepseek_harness 在解析器落地前仅以 monitor 形式出现（Planned/blocked 态），sync 不写 usage 行。
- [ ] `docs/agents/passive-source-candidates.md` 候选表更新（zcode/antigravity/deepseek 三行结论与实现对齐）。
- [ ] CLI 行为变化同步文档：`README.md`、`README.zh-CN.md` 及对应 docs 页（新增来源列表、source-status 输出）。
- [ ] 隐私红线：不持久化任何 prompt/响应文本；fixture 全部合成脱敏；bounded reader 契约测试覆盖新 JSONL 路径（如有）。

## 约束

- ~~deepseek 命中 `docs/agents/passive-parser-onboarding.md` 停止规则~~（二次校验解除：`~/.dsh` 有 15 个真实样本，正常/空两类齐备，官方 token 语义明确）；停止规则仍约束 antigravity 的 R1 剩余三项（跨文件出现率、空/中断样本、脱敏 fixture）勾完才能 start。
- 不装钩子、不写第三方配置（ADR-0011 被动原则不变）。
- 新增依赖约束：antigravity 的 protobuf wire 解码手写零依赖；zcode 复用既有 rusqlite；deepseek_harness 的 zstd 解码**首选 `zstd` crate**（tokscale 参考实现同款，Windows CI 已验证；ruzstd 为备选），任何依赖变化须过 `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md`（含隔离 target 的 MSRV 证明）。
- token 归一化以 `.trellis/spec/llmusage/backend/token-accounting-contracts.md` 为准：内部 input 通道必须非缓存、reasoning 默认诊断不计 total、可信上游 total 权威（无上游 total 的来源为通道求和并在候选表注明）。
