# Research: Sync 解析诊断、样本定位与摘要改进

- Query: 分析 2026-09-26 sync 的 Codex `skipped=13`、Grok `accounting=13 / usage_incomplete`，提出分类、定位和样本省略提示的优化验收。
- Scope: internal；产品代码只读、源日志只读、真实 SQLite 只读；仅写本任务 research 目录。
- Date: 2026-09-26
- Task: `.trellis/tasks/09-26-sync-diagnostics-performance`
- Version: 主会话已核实安装二进制自报 `1.3.0`，工作区为 `dev / 29bde59 / Cargo 1.4.0`。本报告代码行号属于该工作区；未验证安装二进制的精确提交。

## Findings

### 1. 本轮诊断与只读实测

`source_sync_status` 中 Codex、Grok 的 `updated_at` 均为 `2026-09-26T08:33:53Z`。保存的计数、8 条样本及 Codex 偏移与用户输出一致。只读探针于 `2026-09-26T09:02:00Z` 读取该批诊断，并按哈希反查受控源文件。

证据文件：

- `research/parse-probe.py`：使用 SQLite URI `mode=ro&immutable=1` 和 `PRAGMA query_only=ON`。仅在无 WAL 时运行。只输出白名单类型、偏移、长度、布尔标记和数值计数；不输出原始 JSON、prompt、工具参数或工具输出。
- `research/parse-probe-results.json`：16 条样本的脱敏核查结果。数据库 size/mtime 前后相同，前后无 WAL。16 条样本所指文件的 size/mtime 均与已保存 cursor 相同，读取前后也相同。元数据一致不等同于逐字节历史快照校验。

| 来源 | 本轮计数 | 保存样本 | 实测 | 边界 |
| --- | ---: | ---: | --- | --- |
| Codex | skipped 13；其它三类 0 | 8 | 8/8 合法 JSON，均为 `response_item / custom_tool_call_output`，均超过 4 MiB | 另外 5 条未保留逐条明细 |
| Grok | accounting 13；其它三类 0 | 8 | 8/8 为 `turn_completed.usage` 且 `usageIsIncomplete=true`，均来自同一 `updates.jsonl` | 该当前文件有 14 条 turn usage，其中 13 条 incomplete；该计数与本轮总计一致，但只有 8 条有保存的本轮逐条定位证据 |

Grok 对应当前文件没有 JSON 解码错误，也没有 `inputTokens < cachedReadTokens + cacheCreationTokens` 的记录。上游设置 `usageIsIncomplete` 的更深层原因未查明。

### 2. Codex skipped 的来源和影响

`src/parsers/file_state.rs:19` 将单条 JSONL 缓冲限制为 `4 * 1024 * 1024 = 4,194,304` 字节。`src/parsers/codex.rs:701-716` 对超限记录检查 payload/msg 类型：可识别的非 `token_count` 计为 skipped；无法恢复的 `token_count` 和无法识别类型的内容仍计为 oversized。类型预读上限为 8 KiB（`src/parsers/codex.rs:42,730-741`）。

本轮 8 条可定位样本的记录长度如下，长度不含末尾 LF。完整 basename 保存在脱敏结果中。

| 文件日期与时间 | 字节偏移 | 记录字节数 | 类型 |
| --- | ---: | ---: | --- |
| 2026-08-26 17:26:36，第一条 | 10,549,661 | 5,561,228 | custom_tool_call_output |
| 2026-08-26 17:26:36，第二条 | 16,668,520 | 6,432,396 | custom_tool_call_output |
| 2026-08-26 17:29:16 | 1,818,223 | 7,626,824 | custom_tool_call_output |
| 2026-08-26 23:13:59 | 8,667,420 | 4,830,422 | custom_tool_call_output |
| 2026-08-27 00:05:02 | 3,416,553 | 12,220,619 | custom_tool_call_output |
| 2026-08-27 00:37:55 | 2,820,391 | 12,221,330 | custom_tool_call_output |
| 2026-08-29 23:18:59，第一条 | 3,765,408 | 8,107,803 | custom_tool_call_output |
| 2026-08-29 23:18:59，第二条 | 12,455,723 | 8,057,501 | custom_tool_call_output |

这 8 条记录均从换行边界开始，并以 LF 结束。当前内容支持“非用量工具输出超过记录上限”的判断；未观察到这 8 条本身包含 `token_count` 事件。`skipped=13` 本身不能证明丢失 13 条 token 用量记录，也不能据此证明所有历史 token 用量完整。

超限回调在 `src/parsers/codex.rs:98-104,587-594` 返回 `Skipped`，通用 reader 在 `src/parsers/file_state.rs:332-355` 记录空 reason。摘要只显示 `skipped @offset basename`，因此用户无法从摘要得知“超限非用量记录”的原因。

建议保留 4 MiB 内存边界和当前 token 分类，增加闭集原因，例如 `oversized_non_usage_record`。实施顺序必须先修订 reason 与 offset 的显示契约，再添加 Codex reason；否则现有 `else if` 会隐藏当前可见的 offset。不要为减少 skipped 数量而统一提高记录上限或把未知 token 记录降级为 skipped。

### 3. Grok accounting 的来源和影响

`src/parsers/grok.rs:675-703` 仅对 `turn_completed` 中可解析的 usage 记录收集异常。`usageIsIncomplete=true` 映射到 `AccountingAnomaly / usage_incomplete`（布尔读取：`src/parsers/grok.rs:812-815`）。该标记不阻止事件构建与输出（`src/parsers/grok.rs:704-727`）。现有单元测试 `incomplete_usage_still_emits_event` 明确断言事件保留、total 保留、异常计数为 1（`src/parsers/grok.rs:1503-1511`）。本研究未执行该 Rust 测试。

Grok token 规则保持：`totalTokens` 有效时优先；缺失时使用 input+output；input 中减去 cache read/creation；reasoning 保持单独诊断值，不能加到 total；有 turn usage 时不叠加 `_meta.totalTokens` 或 `signals`（`src/parsers/grok.rs:745-809,426-443`；`.trellis/spec/llmusage/backend/token-accounting-contracts.md:97-106`）。

建议在摘要或帮助中说明 `usage_incomplete` 是上游用量完整性标记，已报告的有效用量继续导入。当前证据不支持填补缺失 token、按其它 counters 补算、删除事件或把该信息类诊断变为 parse failure。

一个 usage 记录可能同时触发 `input_below_cache` 与 `usage_incomplete` 两次 `record()`（`src/parsers/grok.rs:686-703`）。因此 `accounting_anomaly_lines` 实际上按诊断记录递增，并不保证等于不同物理行的数量。省略提示应使用“诊断样本”，避免承诺“不同文件数”或“不同事件数”。

### 4. Grok 样本定位丢失有两个独立原因

1. `ParseIssueSample::cli_line` 在 reason 非空时不再输出 offset，见 `src/domain/models.rs:121-129`。本轮 Grok 保存了非零 offset，但人类摘要未显示。
2. Grok 传给解析诊断的是 session 目录哈希，见 `src/parsers/grok.rs:347,414-418`。`sample_basenames` 只建立每个 file cursor 原始文件路径的哈希映射，见 `src/commands/sync.rs:399-419`。Grok cursors 仍逐 sidecar 保存，见 `src/parsers/grok.rs:494-512`。目录哈希无法命中文件哈希映射，basename 因而缺失。

建议将“事件/reset 的 session 归属哈希”与“诊断文件定位哈希”作为两个明确参数。只改诊断文件定位，保持 event key、session id、reset path hash 和全会话重放协议。新样本使用实际 sidecar 文件哈希与该文件内 byte offset。旧样本仍需通过受控 parent-directory 映射或显式的“历史定位不可用”提示兼容，不能伪造文件命中。

新 JSONL 样本可同时显示 reason、非零 offset 和安全 basename。`updates.jsonl` 在不同会话中重名；若需增加短哈希作为消歧标识，必须同步修订现有隐私契约和测试。当前规范明确禁止 CLI 输出 `path_hash`（`.trellis/spec/llmusage/backend/source-sync-contracts.md:498-505`），不能把完整路径或完整哈希直接打印出来。数据库型样本 offset=0 的现有无 `@0` 行为应保持；本轮优化不必扩大到零偏移语义。

### 5. 样本上限合理，省略提示缺失

`MAX_PARSE_ISSUE_SAMPLES=8`（`src/domain/models.rs:74`），`record` 和 `merge` 均遵守这一上限（`src/domain/models.rs:192-233`）。计数继续累加。`sync_summary` 打印所有已保存样本后直接返回（`src/commands/sync_summary.rs:189-228`），`source-status` 同样没有省略提示（`src/commands/source_status.rs:238-246`）。

本轮每个来源有 13 条诊断、8 条样本，均省略 5 条明细。建议以四类计数的饱和和减去有效 sample 数计算提示，显示类似 `samples shown: 8 of 13; 5 omitted`。不增加样本预算，不读取额外正文，不要求持久化全量诊断。保留已有四类统计 JSON 名称和 `total()` 语义。

`ParseIssues::total()` 只统计 malformed+oversized，skipped+accounting 为 informational（`src/domain/models.rs:146-169`）。摘要的 warning 颜色只用于前两类（`src/commands/sync_summary.rs:198-214`）；Doctor 同样只按 fault 计警告（`src/commands/doctor.rs:168-189`）。建议保留该语义，并通过闭集原因与解释减少误读。

### 6. Source-level issue 的持久化兼容建议

Antigravity 详细归因由主会话负责。本研究补充一个实现边界：`record_failure` 同时写运行期 `last_error` 与 `Malformed` 样本（`src/parsers/antigravity.rs:145-149`）；`SourceSyncStats.last_error` 存在（`src/parsers/mod.rs:277-281`），但持久化的 `SourceSyncStatus` 无对应字段。存储代码序列化的是 `status.parse_issues`（`src/store/sync_status.rs:178-200`），读回时也直接反序列化为 ParseIssues（`src/store/sync_status.rs:50-71`）。查询层还有独立读取入口 `src/query/diagnostics.rs:16-44`。

若把 missing database 从 malformed 移出，必须先让独立 source-level issue 可持久化、可重载、可呈现；否则本轮的人类 last_error 消失后，后续 `source-status` / Doctor / query 会失去故障依据。

最低存储改动方向是复用现有 `parse_issues_json` 的 JSON 容器，增加有界、闭集、带 serde default 的 `source_issues`，保留旧四类字段和旧样本的读取；无需新增 SQLite 列或版本迁移。source issue 应携带闭集 code、severity、有限计数和可推导的恢复动作，不保存自由错误文本。其计数和样本预算应与 JSONL record diagnostics 区分。已有 malformed-only 历史 payload 继续可读，不必改写真实历史状态。

这一方向仍需处理 Rust API 兼容：`ParseIssues` 是字段公开的 struct（`src/domain/models.rs:152-160`），并由 `src/lib.rs:53-55` 公开 re-export。直接新增字段会破坏外部 struct literal 的源码兼容；仅加 serde default 无法解决。设计应评估私有持久化 DTO/新增访问器等兼容入口，并连接所有读写入口，避免未知字段被当前 typed read/write 丢弃。现有 public 类型如需变更，必须通过项目规定的 semver 检查，不能把“无 DB migration”误写为“无兼容性影响”。

### 7. 最小复现与可执行验收

| 验收项 | 最小 fixture / 操作 | 可观察结果 |
| --- | --- | --- |
| Codex 超限分类 | 合成 `response_item/custom_tool_call_output`，内容长度 4 MiB+1，后接一个合法 token_count；另测未知类型和不可恢复 token_count | 非用量行 skipped=1 且闭集原因可见；后续 token total 与 cursor 正确；未知类型及不可恢复 token_count 仍 oversized |
| Grok incomplete | 合成一个 turn_completed usage，非零 total、`usageIsIncomplete=true`，独立 JSONL 位置大于 0 | 事件仍导入，token channels不变，accounting=1；摘要同时显示 reason、offset、定位信息 |
| Grok hash 归属 | 两个临时会话，均含 updates.jsonl；各自生成一个诊断 | 诊断定位到各自实际文件；session/reset/event identity 不变；不泄露 workspace 路径 |
| 8 条预算 | 同一来源产生 13 个诊断，再测试 0、8、9 个边界和跨 shard merge | samples最多8；13时明确显示8/13及省略5；计数完整；0时不显示诊断段 |
| 一行多诊断 | 同一 Grok usage 同时 incomplete 且 input低于cache | 两个计数按现有语义保留，摘要不宣称两条不同物理行 |
| 新旧持久化 | 老JSON缺少reason/新source字段；新JSON带source issue；存储→重启/重读→摘要 | 老值默认正确；新source issue不丢失；不需要SQLite migration；独立query入口结果一致 |
| Source failure分类 | 合成缺失的已跟踪Antigravity DB，使用临时home/临时SQLite | 故障有持久source code与恢复动作，保护历史；JSONL malformed仅表述记录解码问题；不能因分类调整使Doctor/status误报健康 |
| 隐私 | 在合成正文放入独特 secret 字串，分别验证人类摘要、NDJSON、持久诊断JSON和交互dashboard payload | secret、全文、完整路径、未经批准的hash不出现；dashboard继续仅暴露允许的计数/状态字段 |

优先复用现有测试：

- Codex：`src/parsers/codex.rs:1550` 超限非用量；`:1581` 完整 token_count 前缀恢复；`:1611` 不可恢复 token_count；`:1470` 附近的 partial-tail/cursor 测试。
- Reader：`src/parsers/file_state.rs:696` 正文不泄露；`:710` 样本数/哈希长度有界。
- Grok：`src/parsers/grok.rs:1504` incomplete 保留事件；`tests/sync/sources/grok.rs:304` 精确计数、幂等、会话重放；`:234` bounded run/full cursor保护；`:4` 缺失sidecar历史保护。
- 摘要：`src/commands/sync_summary.rs:501` 四类计数；`:535` basename隐私；`:568` reason与offset=0兼容。
- 状态/消费者：`src/store/sync_status.rs` round-trip；`src/query/tests/diagnostics_snapshot.rs:34-78` counters-only dashboard；`src/commands/doctor.rs:239-261` informational诊断不变成parse warning。

建议实施阶段运行的命令（本研究未运行）：

```powershell
cargo test --locked --all-features --lib parsers::codex::tests -- --test-threads=1
cargo test --locked --all-features --lib parsers::grok::tests -- --test-threads=1
cargo test --locked --all-features --lib commands::sync_summary::tests -- --test-threads=1
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test query -- --test-threads=1
cargo test --locked --all-features --test cli -- --test-threads=1
python scripts/ci-rust.py
cargo semver-checks --baseline-rev v1.2.0
```

CLI 行为变更需更新 README、README.zh-CN 和相关 docs；跨 Rust/docs 界面按 AGENTS.md 执行 `just ci`。不要在真实 home/DB 上运行复现 sync 或 rebuild。

## Files Found

| 路径 | 职责 |
| --- | --- |
| `src/domain/models.rs` | 诊断模型、四类计数、8条预算、闭集reason过滤、CLI样本格式 |
| `src/parsers/file_state.rs` | 4MiB有界JSONL reader、字节offset、耐久cursor、超限分类 |
| `src/parsers/codex.rs` | Codex超限非用量分类、token_count解析、增量cursor |
| `src/parsers/grok.rs` | session重放、turn usage完整性标记、目录归属哈希 |
| `src/parsers/source_files.rs` | Codex/Grok源发现和Grok两层目录限制 |
| `src/commands/sync.rs` | 根据file cursor映射诊断basename |
| `src/commands/sync_summary.rs` | 人类摘要、四类计数和样本行 |
| `src/commands/source_status.rs` | 持久source状态样本输出 |
| `src/store/sync_status.rs` | parse_issues_json持久化与读取 |
| `src/query/diagnostics.rs` | 独立读诊断JSON并投影交互计数/状态 |
| `src/commands/doctor.rs` | malformed/oversized诊断警告 |
| `tests/sync/sources/grok.rs` | 会话重放、幂等、missing-sidecar和bounded-run集成回归 |

## External References / Related Specs

- 外部仓库更新和比较由主会话/参考实现研究负责。本子研究未执行 git 操作，也未以外部资料推断本轮数据。任务 PRD 指定比较锚点：ccusage `732c7a6362f3d86a4992d2ad7071b6532161a396`；tokscale `1d9a9395418efc6952944b794097935d7d6fa1e8`。
- `.trellis/spec/llmusage/backend/source-sync-contracts.md:464-559`：记录上限、诊断预算、reason/offset/隐私契约；其中 502-505 的 formatter 规则需在优化时明确修订。
- `.trellis/spec/llmusage/backend/token-accounting-contracts.md:45-74,97-106`：Codex/Grok token会计和历史保护边界。
- `.trellis/spec/llmusage/backend/index.md`：源同步跨层检查与 Rust 验收入口。
- `docs/agents/domain.md`、`docs/adr/0007-llmusage-error-surface.md`：域文档约束及公共错误/API兼容背景。

## Caveats / Not Found

- 没有修改产品源码、原始日志或真实用量数据库；没有启动产品sync/rebuild，也没有运行Cargo测试。研究探针执行成功不代表Rust回归测试已经通过。
- 安装1.3.0与工作区1.4.0存在版本边界。真实样本与本轮保存诊断已核对；源码机制与本轮现象一致，仍不能证明安装二进制与HEAD逐行相同。
- 两个来源每次最多保存8条样本。Codex另外5条没有逐条证据；Grok当前对应文件13条incomplete足以解释总计，但保守区分当前文件统计与本轮保存的8条样本。
- `usageIsIncomplete` 的上游深层原因、缺失token数量、这类标记是否会在后续源更新中被修正，均未确定。
- 本轮没有测量分类/格式化耗时，也没有将2.5GB或105.2s写入时间归因到这些诊断。性能归因由独立研究负责。
