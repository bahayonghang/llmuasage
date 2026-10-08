# 让定价目录自动跟上新模型

## Goal

模型和价格都不再靠手写静态价卡。`llmusage sync` 按 tokscale 的规则维护公开价表：某个来源的成功缓存未超过 1 小时就跳过它的网络请求；超过 1 小时才拉取。拉取失败、没有网络或结果不可用时，继续使用该来源上一次成功缓存，不论缓存多旧。已有模型的价格也会随价表变化，而不只是补新模型。这个能力随 `2.0.1` 发布。

`claude-sonnet-5-5`、`claude-haiku-5-5`、`gpt-6.1-sol` 是这条自动路径必须覆盖的例子，不是再手写三行目录。

## Background

当前仓库没有自动更新。

- 内置价卡是 `pricing/static-v2.json`，`schema_version` 为 `2`，身份为 `static-v3`。模型行由人维护。
- 未 pin 的 `static-*` 目录只在内置身份变化时，于下次 `sync` 重算。完整 snapshot 和 overlay 保持 pin。
- `llmusage catalog apply <file>` 合并本地覆盖层。`doctor --refresh-pricing <file>` 只激活一份本地完整 base snapshot。URL 会被拒绝。
- README 约定：所有目录输入都是本地文件，llmusage 不联网拉取价格。这次按用户决定改掉这条。
- 原生 LiteLLM 导入目前不保留长上下文分档。
- 现有 family 匹配会让更具体的新模型误用旧费率。
- 当前 crate、桌面端和 README 版本都是 `2.0.0`。`CHANGELOG.md` 里 `2.0.0` 仍是 Unreleased。本仓库没有 `v2*` tag。

参考实现见 `research/upstream-pricing-sources.md`。ccusage 和 tokscale 的主表都是 LiteLLM；两边都再用 models.dev 补 LiteLLM 没有的模型和长上下文费率。tokscale 另有 OpenRouter。本任务采用 tokscale 的 1 小时跳过，不把 OpenRouter 作为默认同步来源。

## Requirements

1. 每次 `sync` 在解析任何来源之前，对每个价表来源单独决定是否请求。成功缓存的年龄不超过 3600 秒时，不请求该来源。超过 3600 秒，或没有可用缓存时，才发该来源的公开 GET。两个来源是 LiteLLM 的 `model_prices_and_context_window.json`，以及 models.dev 的 `api.json`。不上传用量，不读取账号。
2. 某个来源本次拉取成功后，用新价表替换它的缓存，并让目录价事件按新价重算。已有模型如果在新价表里变价，也要变。缓存未过期而跳过请求时，不重算。
3. 任一来源网络失败、超时、空文档或无法使用时，不覆盖该来源上一次成功缓存，并继续同步。两个来源都失败时，使用仍可用的缓存。
4. 从未成功缓存且这次刷新也失败时，使用随二进制发布的内置目录。内置目录只承担这次冷启动，不再是日常价源。
5. 价表里的新模型按自己的费率和上下文窗口计价，不能落到更宽的旧 family。例子是 Claude / OpenCode 的 `claude-sonnet-5-5` 与 `claude-haiku-5-5`，以及 Codex / OpenCode 的 `gpt-6.1-sol`。
6. 价表已经公布的长上下文分档必须作用在单条事件上。不得把分档收成只有默认价。
7. Parser 继续保存原始模型名。不新增数据源，不改 SQLite schema。source-reported 的正成本仍优先于目录价。
8. `catalog apply` 的用户覆盖层仍按模型 id 覆盖刷新后的基表。刷新不得静默丢掉覆盖层。
9. 价格回退警告不得写进 `sync --json-events` 的 stdout。
10. 测试不得访问真实价表网络。
11. 用户可见版本从 `2.0.0` 升到 `2.0.1`，包括根 crate、桌面端 crate、Tauri 配置、桌面 `package.json` 与 lockfile、中英 README。Changelog 只能有一个当前未发布版本，且该版本是 `2.0.1`。

## Acceptance Criteria

- [ ] AC1 每个价表来源单独计时。缓存年龄不超过 3600 秒时，本次 `sync` 不请求该来源。超过 3600 秒或没有可用缓存时，在解析日志之前最多请求一轮。对应 R1。
- [ ] AC2 拉取成功后，该来源缓存被替换；目录价的新旧事件都按新费率计算。一个已有模型的费率变化能改变它的已落库成本。缓存未过期而跳过请求时，不重算。对应 R2。
- [ ] AC3 来源失败、超时或文档不可用时，该来源的上次成功缓存保持不变，同步仍完成。对应 R3。
- [ ] AC4 没有缓存且刷新失败时，成本使用内置目录，同步不失败。对应 R4。
- [ ] AC5 价表提供上述三个例子时，Claude 或 Codex 以及 OpenCode 都命中例子自己的费率，而不是旧 Sonnet、Haiku 或 GPT family。对应 R5。
- [ ] AC6 价表中的长上下文阈值作用在单条事件上；恰好等于阈值的请求留在阈值以下的档。对应 R6。
- [ ] AC7 原始 `usage_event.model` 不被改写。source-reported 正成本不被目录重算覆盖。对应 R7。
- [ ] AC8 用户覆盖层中的模型 id 在刷新后仍使用覆盖层费率。对应 R8。
- [ ] AC9 `sync --json-events` 的 stdout 不包含价格回退警告。对应 R9。
- [ ] AC10 定价测试使用本地夹具，不连接 LiteLLM 或 models.dev。对应 R10。
- [ ] AC11 版本面都显示 `2.0.1`。Changelog 不同时把 `2.0.0` 和 `2.0.1` 当成当前版本。对应 R11。
- [ ] AC12 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`，以及受影响的定价、catalog、sync 测试通过。文档变更则 `npm --prefix docs run docs:build`。

## Out of Scope

- 把点名模型手写进内置价卡当作实现。
- 默认再拉取 OpenRouter。
- Fast、Batch、Flex、区域加价和工具调用费。
- 拆分 Claude 5 分钟与 1 小时 cache write。
- 新增 parser、数据源或 schema migration。
- 在测试或普通 CI 中访问真实价表网络。
