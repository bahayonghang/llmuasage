# TokenTracker hooks/usage 参考分析与 llmusage 方案 B 设计规格

日期：2026-05-28
状态：方案 B 已获批准；本文是设计规格，不是实施计划。
适用仓库：`D:\Documents\Code\CLI\llmusage`
参考仓库：`ref\TokenTracker`

## 1. 结论摘要

TokenTracker 并不是“给所有 Agents 都加 hook”。它把不同工具按采集能力分成三类：hook、plugin、passive reader，并用一次 `sync` 串联所有 provider parser。这个思路对 llmusage 的价值不在于照搬 provider 数量，而在于先建立“源能力与激活方式”的唯一真源，再让 hook runner、status、parser onboarding 都从该真源派生。

已批准的方案 B 是：

1. 先建设 Source Capability Registry，明确每个 source 的激活方式、可观测状态、parser 质量与隐私边界。
2. 再修正 hook runner 语义，让 hook 只做可靠、低阻塞、source-aware 的触发工作。
3. 最后按安全批次引入 passive readers；每个新 parser 必须有真实样本与回归验收，不能凭 TokenTracker 代码或 README 猜 schema。

## 2. 事实锚点

### 2.1 TokenTracker 的采集模型

TokenTracker README 的支持表显示，它覆盖 Claude Code、Codex CLI、Cursor、Kiro、Gemini CLI、OpenCode、OpenClaw、Every Code、Hermes、Copilot、Kimi、CodeBuddy、Grok Build、Kilo、Roo、Antigravity、Zed、Goose 等工具。其说明把这些源明确拆成三类：

- hook-based：Claude Code、Codex CLI、Gemini CLI、Every Code、CodeBuddy、Grok Build；安装 SessionEnd hook 或 TOML notify。
- plugin-based：OpenCode、OpenClaw；通过目标工具的 plugin/session plugin 机制触发。
- passive readers：Cursor、Kiro、Hermes、Kimi、Copilot、Grok Build、oh-my-pi、pi、Craft Agents、Kilo CLI、Kilo Code、Roo Code、Antigravity、Zed、Goose；不写目标工具配置，只读其已有 SQLite、JSONL、OTEL 或 session log。

关键实现锚点：

- `src/commands/init.js` 负责安装本地 runtime、写 hooks/plugins、检测 passive readers。
- `buildNotifyHandler()` 生成 `notify.cjs`，该脚本只写 signal、按 20 秒 throttle 拉起后台 sync，并尽快 `exit(0)`。
- `notify.cjs` 对 Codex / Every Code 的原 notify 做备份与 runtime chaining，避免覆盖用户原有 notify 后丢行为。
- `src/commands/sync.js` 顺序调用 `rollout.js` 中大量 `parse*Incremental()`，所有 provider 最终追加到 `queue.jsonl` 并维护 `cursors.json`。
- `src/lib/passive-mode.js` 将“hook 未安装但日志存在”标为 passive mode，用于 status 提示用户当前是无 hook 的延迟采集。

### 2.2 TokenTracker 可借鉴与不可照搬之处

可借鉴：

- 激活方式分类，而不是把所有 source 都塞进 install/probe/uninstall hook 语义。
- hook 轻量化：写 signal、throttle、后台 sync、失败静默，避免阻塞被 hook 的 AI 工具。
- 原始 notify 保留与链式调用，特别适用于 Codex 这类 singleton notify 配置。
- passive mode status：用户需要知道“没装 hook 但可以读日志”和“完全没有数据”的区别。
- 每源 cursor / dedup / delta 语义必须显式化。

不可照搬：

- `rollout.js` 与 `sync.js` 是超大集中式实现，provider 真源分散在 README、init、status、sync、rollout 之间；这会削弱 llmusage 现有 Registry/Parser 边界。
- `queue.jsonl` + `cursors.json` 的迁移/修复逻辑复杂，llmusage 已有 SQLite、`SyncShard`、`SyncRunWriter`，应继续沿用更强的原子提交协议。
- TokenTracker 对部分 passive 源只能估算 token 或缺少 prompt/output/cache 拆分；llmusage 不能把这类数据伪装成与 Codex/Claude 同质量。
- README 支持面不等于可验收 parser。llmusage 新源必须有真实本地样本、fixture、sync-twice 与 cursor/rotation/truncation 回归。

### 2.3 llmusage 当前基础与缺口

当前优势：

- `SourceKind`、`SourceParser`、`Integration`、`registry::registered_*()` 已把 parser 与 integration fan-out 收敛到注册表。
- `SyncShard` + `SyncRunWriter::commit_shard` 已把 reset → events → cursor → raw/turn/tool facts 的写入协议固化，适合新增源。
- `source_file` 状态机能支持 rebuild guard 与 missing/deleted_by_user 区分。
- `hook_run` 已有 trigger_state、worker lock、补跑循环，具备可靠触发基础。

主要缺口：

- `Integration` 默认等价于“可安装/可卸载的外部配置”，passive reader 没有自然位置。
- `SourceKind` 同时承担存储 ID、CLI 过滤、别名、UI 名称与能力元数据；随着 passive 源增加会膨胀。
- 目前没有 Source Capability Registry，status、init、sync 与 parser onboarding 缺少同一份能力事实。
- `hook_run` 当前拿到锁后会跑完整 `run_once(app, store, 0)`，更像 foreground full sync；TokenTracker 风格扩展后应改成低阻塞、source-aware、可 throttle 的触发器。
- Codex integration 有 backup/restore，但缺少 runtime chaining 原 notify 的明确产品语义。
- status 需要区分 active、passive、no_data、degraded、estimated，而不是只显示 hook/config 是否存在。

## 3. 目标与非目标

### 3.1 目标

1. 建立一处可测试的 Source Capability Registry，成为 source 支持面、激活方式、status 语义、parser 质量声明的唯一真源。
2. 保留 llmusage 现有 parser / writer / SQLite 架构，不引入 TokenTracker 的 queue 文件模型。
3. 让 hook runner 成为“可靠触发器”，而不是每次 hook 都阻塞式全量同步所有源。
4. 为 passive reader 增加统一准入门槛：真实样本、fixture、cursor/dedup、token 质量声明、隐私边界。
5. 让用户在 CLI/status/dashboard 中看懂每个 source 是 hook、plugin、passive 还是 hybrid，以及数据是否完整、延迟、估算或降级。

### 3.2 非目标

- 不在本设计中一次性支持 TokenTracker 的全部 provider。
- 不改变 `SyncShard` / `SyncRunWriter` 的核心写入协议。
- 不引入云上传、排行榜或账号体系。
- 不把外部 API 抓取作为默认采集策略；本设计聚焦本地 usage artifacts。
- 不根据参考仓库代码猜测闭源工具 schema；没有真实样本的 source 只能停留在候选列表。
- 不把 passive reader 伪装成 hook 已安装，也不把估算 tokens 伪装成精确 usage。

## 4. 方案 B 总体架构

### 4.1 分层边界

新增 Source Capability Registry，但不替代现有 parser registry；它位于 domain/registry 层，提供 source 的静态能力描述。现有 `registered_parsers()` 与 `registered_integrations()` 可以继续存在，但需要被 descriptor 校验或从 descriptor 派生，避免多处 source 真源漂移。

推荐概念模型：

```rust
pub struct SourceDescriptor {
    pub kind: SourceKind,
    pub stable_id: &'static str,
    pub aliases: &'static [&'static str],
    pub display_name: &'static str,
    pub activation: ActivationMode,
    pub capabilities: SourceCapabilities,
    pub quality: UsageQuality,
    pub privacy: PrivacyClass,
}

pub enum ActivationMode {
    Hook(HookDescriptor),
    Plugin(PluginDescriptor),
    Passive(PassiveDescriptor),
    Hybrid(HybridDescriptor),
}
```

这些类型名是设计语言，不要求逐字成为最终代码；实施时可按 Rust 模块边界调整。

### 4.2 `SourceKind` 的角色收敛

`SourceKind` 应继续作为 SQLite/JSON/CLI 的稳定存储 ID，不承载 UI 名称、安装方式、parser 质量等扩展元数据。

设计约束：

- `SourceKind::as_str()` 只表达稳定 ID。
- 别名如 `antigravity -> gemini` 应迁移到 descriptor 的 aliases 或专门解析层，避免 enum match 分散。
- 新增 source 时，必须新增 descriptor；parser 与 activation handler 是否存在由 descriptor 能力声明和测试断言保证。
- 若多个产品共享同一存储 source，需要 descriptor 明确“产品别名/输入家族”和“落库 source”的关系，避免 status 显示和数据聚合混乱。

### 4.3 ActivationMode 语义

#### Hook

适用于目标工具支持命令 hook / notify，且 llmusage 需要写入其配置。Hook source 必须声明：

- 配置路径与 hook 事件类型。
- hook command / notify args 的生成方式。
- 是否 singleton 配置；若是 singleton，必须支持原始配置备份、恢复与 runtime chaining。
- hook 失败是否允许 passive fallback。
- hook 触发时应同步的 source 范围。

#### Plugin

适用于目标工具通过 plugin/session plugin 接入。Plugin source 必须声明：

- plugin 安装位置与 marker。
- install/probe/uninstall 的幂等策略。
- plugin 是否只触发 sync，还是直接产生可解析 usage artifacts。
- 目标工具升级后 plugin 失效时的 status 降级方式。

#### Passive

适用于不写目标工具配置，只读已有文件或 DB。Passive source 必须声明：

- artifact 路径发现规则。
- 是否需要 auth/local token；若需要，缺失时标为 `degraded_auth_missing`，不能视作未安装。
- cursor 粒度：文件 offset、DB row id、mtime/fingerprint、cumulative snapshot delta 等。
- token 质量：精确拆分、累计 delta、总量估算、成本估算不可用等。
- 隐私边界：是否读取 prompt/response 文本、是否只读取 usage 字段、是否需要 raw archive。

#### Hybrid

适用于既有 hook 又需要 passive scan 兜底或补历史的 source。Hybrid source 必须声明 hook 与 passive 的合并规则：

- hook 只触发近期 sync，passive scan 负责补历史或修复漏触发。
- 两路数据必须使用同一 dedup key 或 source_path_hash 策略。
- status 需要能同时显示 hook installed 与 passive logs present。
- hook 缺失但 passive 可读时，显示 passive/degraded，而不是失败。

## 5. Status 与用户语义

Source status 应从 descriptor + probe + parser/cursor 状态组合得出，而不是仅从 integration probe 得出。

推荐状态集合：

| 状态 | 含义 |
| --- | --- |
| `not_detected` | 未发现工具或本地 artifacts。 |
| `configured_hook` | hook 已安装，等待或已经触发。 |
| `configured_plugin` | plugin 已安装并可触发。 |
| `passive_ready` | 不需要 hook，已发现可读 artifacts。 |
| `passive_no_data` | passive source 可探测，但暂未发现 usage artifacts。 |
| `degraded_hook_missing` | 该 source 期望 hook，但 hook 不存在；若日志存在，可继续 passive 延迟采集。 |
| `degraded_auth_missing` | 需要本地 auth/token 才能读取 usage，但凭据缺失或过期。 |
| `estimated` | 当前 parser 只能估算总量或成本，不能提供完整拆分。 |
| `error` | probe/parser 最近一次失败，需要保留错误摘要与时间。 |

Status 输出应同时显示：激活方式、是否会写外部配置、最后一次 sync、最后一次 hook signal、cursor 位置摘要、数据质量。这样用户能理解“已支持但没数据”“有日志但没 hook”“只估算”这些差异。

## 6. Hook runner 设计

### 6.1 目标语义

Hook runner 应保证：

- hook 入口尽快返回，不让 Claude/Codex/Gemini 等工具因 usage sync 卡住。
- 信号先落库，sync 可失败但 signal 不丢。
- 多次 hook 触发可合并，避免短时间内重复 full sync。
- hook source-aware：Codex hook 默认只同步 Codex 及明确声明的 hybrid/passive companion，不应每次同步所有 source。
- 对 singleton notify 的目标，llmusage 接管后仍能链式调用用户原 notify。

### 6.2 与当前 `hook_run` 的关系

当前 `hook_run` 已有正确的基础动作：写 `trigger_state`、拿 worker lock、`recover_running_runs`、最多 3 轮 snapshot 补跑。问题是拿到锁后调用 `run_once(app, store, 0)`，语义上是完整 sync。

方案 B 不推翻这套可靠性结构，而是调整 sync 调用面：

- trigger_state 继续作为 durable signal。
- lock 继续防止并发 writer。
- snapshot 补跑继续吸收运行期间的新 signal。
- `run_once` 需要支持 source filter 或 trigger plan，由 descriptor 决定 hook source 对应的 parser 集合。
- 增加 debounce/throttle 策略，短窗口内只记录 signal 或安排延迟 worker。
- hook 入口可拆成“快速 signal 写入”和“后台 worker 执行”两段；是否 detach 需要按 Windows/Unix hook target 分别验证。

### 6.3 Codex notify chaining

对于 Codex 这类 singleton notify 配置，install 时若发现用户已有 notify，应备份原值；hook runtime 执行 llmusage 自身逻辑后，如果原 notify 不是 llmusage 自身且仍可安全执行，应转发原始 payload。

设计约束：

- uninstall 恢复原 notify；如果用户在 llmusage 安装后手动改过 notify，不能盲目覆盖。
- chaining 必须避免递归调用自身。
- 原 notify 执行失败不应导致 llmusage hook 失败，也不应阻塞目标工具退出。
- chaining 行为应在 status 中可诊断，例如显示“original notify chained”。

## 7. Passive reader 准入合同

新增 passive source 前必须满足以下合同。

### 7.1 样本与隐私

- 至少一组真实本地样本，覆盖正常 session、空 session、异常/中断 session。
- 样本必须脱敏，并说明是否包含 prompt/response 文本。
- parser 默认只提取 usage 所需字段；如需要 raw archive，必须记录字段级隐私理由。

### 7.2 Cursor 与幂等

- 明确 cursor key 命名，不能与已有 source 冲突。
- 文件类源必须处理 fingerprint、size、mtime、tail signature、truncation、rotation。
- DB 类源必须声明 row id / timestamp / cumulative snapshot 的推进规则。
- sync twice 必须第二次零新增或只新增真实 delta。
- 删除/移动源文件时要与 `source_file` state machine 一致。

### 7.3 Token 语义

- 明确 input、cache read、cache creation、output、reasoning、total 的来源。
- 若只能得到 total，应设置质量为 estimated/total-only，不填伪拆分。
- 若只能从累计快照推导 delta，必须处理重置、回退、跨模型累计与重复 session。
- cost 计算只能在模型价格可识别且 token 语义明确时启用。

### 7.4 验收测试

每个新 parser 至少需要：

- fixture parse 单测：样本到标准 `UsageEvent` / bucket 的映射。
- sync-twice 集成测试：第二次不重复入库。
- cursor 回归：truncation / rotation / deleted file 或 DB 重建场景。
- rebuild guard 测试：source_file 与 cursor 行为符合预期。
- status/probe 测试：not_detected、passive_ready、degraded 或 estimated 能被准确表达。

## 8. 分阶段落地边界

这不是实施计划，仅定义后续 planning 的阶段边界。

### 阶段边界 A：Descriptor 先行

只为现有 Codex、Claude、OpenCode、Gemini 建 descriptor 和 registry invariant。该阶段不新增 provider，不改变 parser 行为。成功标准是 status/init/sync 可以读取同一份 source metadata，并通过测试防止 parser registry 与 descriptor 漂移。

### 阶段边界 B：Hook runner 语义修正

围绕现有 hook source 修正触发行为：source-aware sync、throttle/debounce、Codex original notify chaining 语义、status 诊断。该阶段仍不新增 passive parser，以降低变量数量。

### 阶段边界 C：Passive probe 与 status

先加入 passive capability/probe/status，不急于落库 usage。目标是让用户看到“发现了 Kimi/Copilot/CodeBuddy 等 artifacts，但 parser 尚未启用或样本不足”的透明状态。没有真实样本的 source 只能显示候选/未启用，不进入 parser registry。

### 阶段边界 D：小批量 parser onboarding

按样本质量选择最小批次。优先考虑满足以下条件的 source：

- artifact 格式稳定且本地可读。
- token 字段明确，不需要凭空估算拆分。
- 与现有 parser 形态接近，例如 Claude-like JSONL 或 OpenCode-like SQLite。
- 能提供脱敏 fixture 并通过准入合同。

候选名称可以来自 TokenTracker 支持表，但最终进入 llmusage 取决于真实样本与验收测试，而不是参考仓库声称支持。

## 9. 风险与修正

| 风险 | 修正 |
| --- | --- |
| SourceKind enum 随 provider 增加持续膨胀 | 用 descriptor 承载别名、显示名、能力；enum 只保留稳定落库 ID。 |
| passive source 数据质量不一致 | `UsageQuality` 强制声明 precise / total-only / estimated / unsupported-cost。 |
| hook 触发 full sync 导致工具退出变慢 | hook fast path + worker lock + source filter + throttle。 |
| singleton notify 覆盖用户配置 | backup/restore + runtime chaining + recursion guard。 |
| 多源 registry 漂移 | descriptor、parser registry、activation handler 增加 invariant tests。 |
| 参考仓库 schema 与真实工具版本不一致 | 真实样本优先；无样本不进入 parser。 |
| status 过度乐观 | 明确 no_data、degraded、estimated，避免“auto supported”误导。 |

## 10. 方案验收标准

后续实施完成后，应能证明：

1. 新增 source metadata 不需要修改 sync/status/init 的多处分散 match。
2. 现有四个 source 的 sync 行为保持兼容，`SyncShard` 写入协议不退化。
3. hook-run 在短时间多次触发时不会并发 writer，也不会无条件同步所有 source。
4. Codex 原 notify 在 llmusage 接管后仍可被安全链式调用或被明确诊断为不可链式调用。
5. passive source 在无 parser、无数据、凭据缺失、估算质量时都能给出不同状态。
6. 每个新增 parser 都有真实 fixture、sync-twice、cursor/rebuild guard、token 语义测试。

## 11. 最终设计决定

采用方案 B：先能力注册表，再 hook runner，再 passive readers。

拒绝直接照搬 TokenTracker 的“扩大 parser 列表 + 单个 sync.js 串联”路径，因为这会破坏 llmusage 已建立的 SourceParser、Integration、Registry 与 SyncShard 边界，也会把低质量/估算型 passive 数据过早混入精确 usage 统计。

拒绝先批量新增 passive parsers，因为没有 Source Capability Registry 时，status、质量声明、cursor 合同和用户语义会散落到各 parser 与命令层，后续维护成本会快速上升。
