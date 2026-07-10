# 实施计划 - 模型目录配置化与 GPT-5.6

## Gate 0 - 开始实施前

- [x] 由用户审阅并批准 `prd.md`、`design.md` 和本计划。
- [x] 批准后运行
      `python ./.trellis/scripts/task.py start 07-10-configurable-model-catalog`，在此之前不改生产代码。
- [x] 加载 `trellis-before-dev`，重新读取 pricing、store、commands 层规范及相关 ADR。
- [x] 检查工作树并保留与本任务无关的改动；记录实现开始时的基线。
- [x] 重新核对 OpenAI 官方 GPT-5.6 模型页和 pricing 页。价格、上下文窗口或 272K
      规则若有变化，先回到规划阶段更新 PRD/设计。

## Step 1 - Catalog V2 Schema、规范化与校验

- [x] 在 `src/query/pricing_catalog.rs` 及必要的同层子模块中加入 v2 wire schema：
      `kind`、稳定 `id`、多 `sources`、显式 `exact`/`family` matcher、default rate、tiers
      和 `remove_models`。
- [x] 将 wire schema 与运行时编译结构分离；所有 base、overlay、内部 v1 和 LiteLLM 输入
      最终走同一套规范化、matcher 编译和全量校验路径。
- [x] 实现确定性的 matcher 优先级：exact 优先于 family，同模式最长 matcher 优先；同分歧义、
      重复规范化 matcher、空值、非法 source/model ID 必须拒绝加载。
- [x] 校验所有费率有限且非负，context window 与 tier 阈值为正，tier 阈值严格递增。
- [x] 保留现有内部 v1 完整快照和 native LiteLLM 文件适配；不得为缺失 tier 猜测价格。
- [x] 新增 `PricingCatalog::embedded()`；保留 `static_v1()` 兼容 wrapper，生产调用迁移到
      `embedded()`。
- [x] 为可扩展配置提供构造器，减少未来增加字段时对 Rust struct literal 的破坏。

验证：

```powershell
cargo test pricing_catalog
cargo test snapshot
```

回滚点：本步骤只建立 schema/编译器与兼容适配；若 v1/LiteLLM 回归未解决，不进入静态数据迁移。

## Step 2 - Embedded Static V2 与 GPT-5.6 数据

- [x] 将嵌入资源升级为 `pricing/static-v2.json`，把现有模型无损迁移到 v2 数据契约。
- [x] 为 `codex`、`opencode` 添加 `gpt-5.6-luna`、`gpt-5.6-terra`、
      `gpt-5.6-sol`，均使用 exact matcher 和 1,050,000 context window。
- [x] 为 Sol 增加 exact alias `gpt-5.6`；不得使用会吞掉无关 ID 的宽泛新 matcher。
- [x] 写入 short/long 四通道官方费率，long tier 使用
      `prompt_tokens_above = 272000`。
- [x] 保留现有 GPT/O/Claude/Gemini 等模型行为，并证明 exact GPT-5.6 行优先于旧 `gpt-5`
      family fallback。
- [x] 添加数据契约测试：正向 source/model、`gpt-5.6` alias、大小写/规范化行为及
      `not-gpt-5.6-*` 等负例。

验证：

```powershell
cargo test pricing_catalog_loads
cargo test gpt_5_6
```

回滚点：v2 loader 可独立保留；GPT-5.6 行和 embedded 资源迁移可一起撤回。

## Step 3 - 按 Usage Event 选择 Tier 并记录审计信息

- [x] 扩展 `src/query/pricing.rs`，以单条 `UsageTokens` 的
      `input + cache_read + cache_creation` 选择最高命中 tier。
- [x] 明确边界：272,000 使用 short，272,001 使用 long；不得使用 `total_tokens` 或
      30 分钟 bucket 总量判断。
- [x] 使用选中 tier 的 input/cache/cache-write/output/reasoning 费率计算
      `cost_with_cache_usd` 和 `cost_without_cache_usd`。
- [x] 在 `pricing_rate` 中稳定序列化 `model_id`、tier 名称、prompt tokens、阈值和实际四通道费率，
      使历史事件可审计。
- [x] 保持 reasoning included/separate 语义；未命中模型继续返回 unpriced，不做家族价格猜测。
- [x] 添加 Luna/Terra/Sol short、边界、long、cache-write 和 alias 的精确成本测试。
- [x] 添加同一 bucket 混合 short/long 事件测试，断言 bucket 仅汇总事件成本并产生现有 mixed
      审计语义，不二次应用阈值。

验证：

```powershell
cargo test pricing
cargo test recompute_costs
cargo test bucket
```

回滚点：tier 计算必须在静态 v2 激活前通过逐事件和 bucket 回归测试。

## Step 4 - 双层目录合并、内容寻址与激活服务

- [x] 实现 whole-model overlay 合并：先校验 base，再执行严格 `remove_models`，随后按稳定 ID
      完整替换或追加，最后对 effective catalog 做全量歧义和费率校验。
- [x] 使用 SHA-256 内容摘要生成 `base-*`、`overlays/overlay-*`、`effective-*` 文件名；声明的
      `version` 只用于展示，绝不参与路径拼接。
- [x] 扩展 `LLMUSAGE_HOME/pricing/` 与 meta 读写，保存 active/base/overlay/effective 身份；
      保留旧 `pricing/<version>.json` 读取兼容。
- [x] 抽出一个被 catalog CLI 与 `doctor --refresh-pricing` 共用的激活/重算服务，避免两套
      元数据切换逻辑。
- [x] 激活时先校验并落盘，再逐事件重算，最后在 bucket reconcile 事务中切换完整 meta；
      任一步失败不得让下次启动静默使用 embedded。
- [x] `Store::active_pricing_catalog` 必须 fail closed：meta 指向文件缺失、摘要/版本不符或文件
      无效时返回错误。
- [x] 实现 embedded 自动升级：只升级没有用户 snapshot/overlay 的旧 static 目录；完整 snapshot
      与基于旧 embedded 的 overlay 保持 pinned。
- [x] 为 merge、remove、replacement、重复 apply、重启恢复、损坏文件、旧路径兼容、自动升级与
      pinned 行为添加 store 级测试。

验证：

```powershell
cargo test active_pricing_catalog
cargo test catalog_overlay
cargo test pricing_catalog_upgrade
```

回滚点：meta 切换之前旧活动目录保持有效；失败产生的未引用 digest 文件允许保留用于审计。

## Step 5 - `catalog` CLI 与 Doctor 兼容入口

- [x] 在 `src/commands/catalog.rs` 和 `src/commands/mod.rs` 添加：
      `catalog apply <PATH>`、`catalog status [--json]`、`catalog reset`。
- [x] `apply` 只接受本地普通文件；若已有 overlay，始终对记录的 base 合并，禁止叠到旧
      effective 上。
- [x] `status` 的人读/JSON 输出稳定区分 base、overlay、effective、schema、model count、
      source rule count 和 `rebase_available`。
- [x] `reset` 有 overlay 时恢复记录的 base 并清理 overlay meta；无 overlay 时幂等，旧 static
      可升级到当前 embedded。
- [x] `doctor --refresh-pricing` 继续接收内部 v1/LiteLLM 完整 snapshot，将其激活为新 base，
      并显式清除已有 overlay。
- [x] 命令输出失败时包含输入路径和验证上下文，但不得泄露本地目录内容。

验证：

```powershell
cargo test --test report_commands catalog
cargo test --test report_commands refresh_pricing
cargo run -- catalog --help
```

回滚点：新 `catalog` 命令可整体移除，旧 doctor 行为仍须由共享服务兼容测试保证。

## Step 6 - 跨层集成与历史重算回归

- [x] 使用隔离 `LLMUSAGE_HOME` 添加 CLI 集成测试，覆盖 apply、status JSON、进程重启、再次
      apply、reset、无覆盖 reset、无效 overlay 和已选目录损坏。
- [x] 通过 Codex 与 OpenCode request-scoped fixture 导入 GPT-5.6 事件，断言 parser 保留原始
      model ID，事件获得正确 tier、pricing source/rate 和非零成本。
- [x] 断言 `sync`/重算后 `usage_event` 与 `usage_bucket_30m` 的成本、status、source、mixed rate
      一致，且 bucket 成本等于事件成本之和。
- [x] 增加 context-pressure 测试，三个模型使用 1,050,000 窗口；未知模型仍计入 unknown/unpriced。
- [x] 回归现有内部 v1、native LiteLLM、Claude cache creation、Gemini/GPT/O matcher、完整 snapshot
      pinning 和 doctor 流程。
- [x] 检查大型 fixture 下分页重算失败后重试的幂等性，不要求把全库重算改成单个长事务。

验证：

```powershell
cargo test --test local_flow gpt_5_6 -- --test-threads=1
cargo test --test report_commands catalog -- --test-threads=1
cargo test context_pressure -- --test-threads=1
```

回滚点：跨层测试失败时先回退到对应 schema、定价或激活步骤修复，不以放宽断言绕过契约。

## Step 7 - 中英文文档与 Trellis 契约

- [x] 更新 `README.md`、`README.zh-CN.md`，加入 catalog 生命周期入口和本地文件边界。
- [x] 更新 `docs/reference/cli.md` 与 `docs/zh/reference/cli.md`，记录 apply/status/reset、输出、
      重算影响、幂等 reset 与错误行为。
- [x] 更新架构和安全中英文页面，说明 embedded + overlay 优先级、whole-model replacement、
      内容寻址、无远程下载及回滚语义。
- [x] 更新 `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`，把 v2 schema、matcher
      优先级、逐事件 tier、snapshot 兼容、fail-closed 激活写成可执行契约。
- [x] 文档示例使用最小 overlay，明确 `version` 不控制文件名，且 `doctor --refresh-pricing`
      是完整 base snapshot 而非增量覆盖。

验证：

```powershell
npm --prefix docs run docs:build
rg -n "catalog (apply|status|reset)|static-v2|overlay" README.md README.zh-CN.md docs .trellis/spec
```

## Step 8 - 最终质量门与人工审阅

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test -- --test-threads=1`
- [x] `npm --prefix docs run docs:build`
- [x] `git diff --check`
- [x] 审查 `pricing/static-v2.json` 中 GPT-5.6 六组 source 展开结果、费率小数和上下文窗口。
- [x] 审查所有活动目录路径均来自 digest/受控相对文件名，且用户声明 version 不进入路径。
- [x] 审查所有阈值判断都发生在 `usage_event` 定价路径，bucket/report 中没有重复判断。
- [x] 审查新增普通模型只需修改目录数据及测试，不需要再扩展生产 Rust alias 分支。
- [x] 运行 `trellis-check`，修复发现后重复相关 gate，再进入 spec/commit/finish-work 流程。

## 风险文件

- `src/query/pricing_catalog.rs`：schema 兼容、matcher 决议和目录校验核心。
- `src/query/pricing.rs`：历史成本与审计字段的计算核心。
- `src/store/sync_writer.rs`、`src/store/mod.rs`：活动目录选择、重算和事务边界。
- `src/commands/doctor.rs`、`src/commands/mod.rs`、新增 catalog command：用户可见生命周期。
- `pricing/static-v2.json`：所有内置模型的单一事实源。
- SQLite meta 与 pricing 目录文件：升级、pinning、失败恢复和路径安全边界。

## 整体回滚策略

若双层激活在实施中无法达到 fail-closed、可重启恢复和幂等 reset 的验收标准，停止发布新
`catalog` 命令；保留通过测试的 v2 loader/tier 定价仅限 embedded 路径，回到规划阶段重新拆分
持久化工作。不得以静默回退 embedded 或在 bucket 总量上近似 272K 规则作为临时方案。
