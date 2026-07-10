# 模型目录配置化并新增 GPT 5.6 系列

## Goal

让 llmusage 的模型识别、定价和上下文窗口覆盖由稳定的数据契约驱动，
减少新增模型时跨 Rust 代码、测试和静态资源的重复修改；同时完整支持
`gpt-5.6-luna`、`gpt-5.6-terra`、`gpt-5.6-sol`。

## User Value

- 新模型主要通过修改目录数据加入，不再依赖修改匹配器和别名 Rust 分支。
- 用户看到的新模型成本和上下文压力使用模型自身数据，而不是误用宽泛 GPT-5 默认值。
- 无法精确表达的价格规则必须显式暴露或拒绝，不能静默产生看似精确的错误成本。

## Confirmed Facts

- 当前内置目录已经位于 `pricing/static-v1.json`，由
  `PricingCatalog::static_v1()` 编译进二进制。
- 当前运行时支持 `llmusage doctor --refresh-pricing <file>` 导入本地
  LiteLLM/内部格式快照，但快照作为完整活动目录使用，不是对内置目录的增量覆盖。
- 模型匹配仍包含 Rust 侧的规范化、别名和家族前缀规则；新增特殊模型或定价规则时
  仍可能需要同时修改 JSON、Rust 结构、计算逻辑和多个硬编码测试。
- OpenAI 官方模型页确认三个模型均有 1,050,000 token 上下文窗口、128,000
  最大输出，并支持 reasoning tokens。
- 官方基础价格（每百万 token，输入/缓存输入/输出）为：
  - `gpt-5.6-luna`: `$1.00 / $0.10 / $6.00`
  - `gpt-5.6-terra`: `$2.50 / $0.25 / $15.00`
  - `gpt-5.6-sol`: `$5.00 / $0.50 / $30.00`
- 三个模型的 cache write 均按未缓存输入费率的 1.25 倍计费。
- 输入超过 272K token 时，整个请求按 2 倍输入价格和 1.5 倍输出价格计费；
  当前 `PricingEntry` 的单一固定费率无法表达该规则。
- `gpt-5.6` 是 `gpt-5.6-sol` 的官方别名。

## Official Sources

- https://developers.openai.com/api/docs/models/gpt-5.6-luna
- https://developers.openai.com/api/docs/models/gpt-5.6-terra
- https://developers.openai.com/api/docs/models/gpt-5.6-sol

## Requirements

- 保持 parser 写入的原始模型标识不变；目录只负责识别、定价和上下文窗口。
- 将模型别名和模型特有价格行为纳入可校验的数据契约，避免新增模型时扩展
  `pricing_aliases()` 之类的硬编码分支。
- 目录加载必须拒绝重复匹配器、无效费率、无效阈值和会导致歧义的条目。
- 内置目录和本地导入目录必须使用同一套 schema、校验和计算路径。
- 采用双层目录：随二进制发布的内置基础目录，加上 `~/.llmusage/` 下由用户
  显式导入并激活的增量覆盖；覆盖层不得要求复制完整基础目录。
- 覆盖层必须有确定的合并优先级、冲突错误、版本/来源记录、成本重算和回滚到
  内置目录的操作路径。
- 提供 `llmusage catalog apply <PATH>`、`llmusage catalog status [--json]`、
  `llmusage catalog reset`；保留 `doctor --refresh-pricing` 作为完整基础快照导入的
  兼容入口。
- 为三个 GPT-5.6 模型加入 `codex` 与 `opencode` 来源覆盖，并支持官方
  `gpt-5.6` 到 Sol 的别名。
- 成本计算必须覆盖基础价格、cache write 和超过 272K 输入后的长上下文价格。
- 长上下文阈值按单条 request-scoped `usage_event` 的提示 token 总量判断；
  30 分钟桶和报表只能汇总已计算的事件成本，不得对聚合 token 再应用阈值。
- 上下文压力必须使用 1,050,000 token 窗口；未知模型仍保持 unknown/unpriced，
  不允许用家族默认值猜测。
- 更新面向维护者和用户的中英文文档，说明目录格式、优先级、刷新/覆盖行为和回滚方式。

## Acceptance Criteria

- [x] 新增普通模型只需修改目录数据与对应契约测试，不需要修改生产 Rust 匹配代码。
- [x] 内置目录可精确识别三个 GPT-5.6 模型及 `gpt-5.6` 别名，且不会误匹配
  `not-gpt-5.6-*` 等无关标识。
- [x] 三个模型的基础费率、cache write 费率、长上下文倍率和 1,050,000
  上下文窗口均有自动化断言。
- [x] 272K 边界上下的成本测试证明倍率只在规则命中时生效，并记录实际采用的费率规则。
- [x] `pricing_rate` 可审计地记录基础费率、命中的长上下文规则及实际倍率。
- [x] `sync`/重算后的事件与 30 分钟桶包含正确的 `pricing_status`、
  `pricing_source`、`pricing_rate` 和成本。
- [x] 无效目录在启用或导入前失败，已选择目录加载失败时不静默回退。
- [x] 用户可以只声明新增/替换条目来覆盖内置目录，无需升级二进制或复制完整目录。
- [x] 激活覆盖、重启后继续使用覆盖、移除覆盖并回到内置目录均有集成测试。
- [x] `catalog status` 的人读与 JSON 输出均能区分基础、覆盖和最终生效目录。
- [x] 现有 Claude/Gemini/GPT/O 系列目录行为和 LiteLLM 快照兼容性不回归。
- [x] `cargo fmt --check`、严格 Clippy、串行完整测试、文档构建和
  `git diff --check` 通过。

## Out Of Scope

- 从远程 URL 自动下载或后台更新价格目录。
- 改写 parser 已存储的原始模型名称。
- 为模型能力、速率限制或 API endpoint 建立通用产品目录。
- 追溯修正无法从现有聚合数据还原的逐请求定价事实。

## Notes

- 该任务是复杂任务，规划完成前需要补齐 `design.md` 与 `implement.md`。
- 用户已确认采用“内置基础目录 + 用户增量覆盖”的双层方案。
- 用户已确认采用独立 `catalog apply/status/reset` 生命周期，并保留现有 doctor 入口。
