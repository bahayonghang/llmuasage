# 模型目录配置化设计

## 1. Design Summary

本任务把当前“静态价格表 + Rust 特例”升级为一个可版本化、可校验、可分层的
模型目录。运行时仍只消费一个编译后的 `PricingCatalog`，但目录来源改为：

```text
内置基础目录 / 完整本地快照
              + 可选用户覆盖层
              -> 规范化与合并
              -> matcher/费率校验
              -> 生效目录
              -> 逐 usage_event 定价
              -> usage_bucket_30m / 报表汇总
```

不引入远程更新，不修改 parser 保存的原始模型 ID，不把模型能力列表扩展到定价
和上下文窗口之外。

## 2. Current Problems

1. `pricing/static-v1.json` 已经是数据文件，但 matcher 只有隐式的
   exact/prefix 行为，无法安全表达 `gpt-5.6` 只精确别名到 Sol。
2. `pricing_aliases()` 仍包含模型特例，新增别名可能要求改 Rust。
3. `PricingEntry` 只有一组固定费率，无法表达按单请求提示长度切换的长上下文价格。
4. `doctor --refresh-pricing` 只能切换完整目录；用户要增加一个模型必须复制整份目录。
5. `pricing_catalog_version` 只有一个指针，无法区分基础目录、覆盖层和最终生效目录。
6. 活动文件名直接由声明版本推导，不适合继续承载不受信任的本地配置标识。

## 3. Catalog V2 Contract

### 3.1 Top-Level Shape

```json
{
  "schema_version": 2,
  "kind": "base",
  "version": "static-v2",
  "models": []
}
```

- `schema_version`: 目录格式版本，v2 固定为 `2`。
- `kind`: `base` 或 `overlay`。完整快照是 base，增量文件必须是 overlay。
- `version`: 人读版本，只用于展示和审计；磁盘文件名使用内容 SHA-256，不直接使用该值。
- `models`: 模型定义。base 内 ID 唯一。
- overlay 可额外声明 `remove_models: [id...]`。

### 3.2 Model Definition

```json
{
  "id": "gpt-5.6-luna",
  "sources": ["codex", "opencode"],
  "matches": [
    { "value": "gpt-5.6-luna", "mode": "exact" }
  ],
  "rates": {
    "default": {
      "input_per_mtok": 1.0,
      "cached_per_mtok": 0.1,
      "cache_creation_per_mtok": 1.25,
      "output_per_mtok": 6.0
    },
    "tiers": [
      {
        "name": "long_context",
        "prompt_tokens_above": 272000,
        "input_per_mtok": 2.0,
        "cached_per_mtok": 0.2,
        "cache_creation_per_mtok": 2.5,
        "output_per_mtok": 9.0
      }
    ]
  },
  "context_window": 1050000
}
```

Key decisions:

- `id` 是覆盖/删除的稳定键，不等于 parser 必须存储的模型名。
- `sources` 允许一份模型数据扩展到多个 llmusage 来源，避免复制价格。
- matcher `mode` 只有 `exact` 与 `family`：
  - `exact` 仅匹配规范化后完全相等的模型 ID。
  - `family` 保留当前受边界保护的 dash/dot 变体匹配。
- 同一来源下 exact 优先于 family；同模式下最长 matcher 优先。
- 任意同分歧义、重复规范化 matcher、空 matcher 均拒绝加载。
- tier 保存官方明确费率，不只保存倍率；这样 `pricing_rate` 可直接审计，未来也能表达
  非线性或不同比例的价格表。
- tier 按 `prompt_tokens_above` 严格递增，选择命中阈值最高的一档。
- 提示 token 定义为非缓存输入、cache read、cache creation 三个输入通道之和。
- 费率必须是有限且非负数；上下文窗口和阈值必须大于 0。

### 3.3 GPT-5.6 Rows

三个模型都用于 `codex` 与 `opencode`，上下文窗口均为 1,050,000：

| Model | Short input/cache/write/output | Long input/cache/write/output |
| --- | --- | --- |
| Luna | 1.00 / 0.10 / 1.25 / 6.00 | 2.00 / 0.20 / 2.50 / 9.00 |
| Terra | 2.50 / 0.25 / 3.125 / 15.00 | 5.00 / 0.50 / 6.25 / 22.50 |
| Sol | 5.00 / 0.50 / 6.25 / 30.00 | 10.00 / 1.00 / 12.50 / 45.00 |

Luna/Terra/Sol 名称使用 exact matcher。Sol 额外声明 exact matcher `gpt-5.6`。
现有宽泛 `gpt-5` family 规则保留作为旧模型 fallback，但 exact 规则优先，不能吞掉
三个新模型。

官方依据：

- https://developers.openai.com/api/docs/models/gpt-5.6-luna
- https://developers.openai.com/api/docs/models/gpt-5.6-terra
- https://developers.openai.com/api/docs/models/gpt-5.6-sol
- https://developers.openai.com/api/docs/pricing

## 4. Compatibility Adapters

### 4.1 Existing Internal V1 Files

缺少 `schema_version` 且含 `models` 数组时继续按当前内部格式读取：

- `source` 转为单元素 `sources`。
- 字符串 `matchers` 转为 legacy `family` 规则，保持现有行为。
- flat rate fields 转为 `rates.default`，tiers 为空。
- 缺少稳定 ID 时使用规范化的首个 matcher + source 生成兼容 ID；这只用于完整快照，
  status 应标记为 legacy。

### 4.2 Native LiteLLM Files

继续支持当前 LiteLLM 对象格式和费率字段转换。native model ID 生成稳定 ID；provider
映射到现有 llmusage source。LiteLLM 未提供的分层价格不猜测。

### 4.3 Rust Library Surface

- 新增 `PricingCatalog::embedded()`，内部调用迁移到该名称。
- 保留 `PricingCatalog::static_v1()` 作为兼容 wrapper，返回当前嵌入目录并标记 deprecated。
- `PricingEntry` 需要承载 matcher 模式和分层费率。为减少以后再次破坏 struct literal，
  本次增加构造器并将可扩展配置结构标为 `#[non_exhaustive]`；现有查找、上下文窗口、
  `load_snapshot` 和成本入口保持。
- `PricingStatus::Static/Snapshot/Unpriced` 不新增枚举值；用户合并目录仍归 Snapshot，
  具体来源由 effective catalog version 与 catalog status 输出表达。

## 5. Overlay Merge

合并始终在规范化的模型定义上完成：

1. 读取并校验 base。
2. 校验 overlay 的 `kind=overlay`、版本和自身 ID 唯一性。
3. 先执行 `remove_models`；引用不存在的 ID 视为配置错误，防止拼写错误静默通过。
4. overlay 中同 ID 定义完整替换 base 定义；新 ID 追加。
5. 展开 `sources`，编译 matcher 并对最终目录运行全量歧义/费率校验。
6. 用规范化后的最终文档计算 SHA-256，生成不可碰撞的 effective ID 和文件名。

覆盖是 whole-model replacement，不做字段级深合并。这样不会留下“继承了旧模型一半费率”
的隐式状态，覆盖文件也能独立审阅。

## 6. Activation And Persistence

### 6.1 Files

目录继续位于 `<LLMUSAGE_HOME>/pricing/`：

- `base-<sha256>.json`: 新导入的完整基础快照。
- `overlays/overlay-<sha256>.json`: 原始、已校验的用户覆盖。
- `effective-<sha256>.json`: 规范化、合并后的完整生效目录。

所有文件名由内容摘要产生；声明的 version 不参与路径拼接。旧版
`pricing/<version>.json` 仅作为兼容读取 fallback。

### 6.2 Meta Keys

- `pricing_catalog_version`: 当前 effective version，保留现有语义。
- `pricing_catalog_file`: 当前非内置目录的相对安全文件名。
- `pricing_catalog_base_version` / `pricing_catalog_base_file`: 覆盖激活时保存 base。
- `pricing_catalog_overlay_version` / `pricing_catalog_overlay_file`: 当前覆盖。

无覆盖时删除 base/overlay 辅助键；当前目录本身就是 base。

### 6.3 Commands

`llmusage catalog apply <PATH>`:

1. 拒绝 URL 和非文件路径。
2. 如果已有覆盖，使用记录的 base，而不是在旧 effective 上再次叠加。
3. 校验、合并、写入 content-addressed 文件。
4. 逐事件重算，最终事务同时更新 bucket 和所有 activation meta。
5. 输出 base/overlay/effective/updated count。

`llmusage catalog status [--json]`:

- 只读显示 active、base、overlay、schema、模型数、展开后的 source rule 数。
- 覆盖基于旧嵌入目录时显示 `rebase_available`，不静默重写用户配置。

`llmusage catalog reset`:

- 有覆盖时恢复记录的 base、重算并清理覆盖 meta。
- 无覆盖时幂等成功；若当前是旧嵌入版本，则升级到当前 embedded 并重算。

`doctor --refresh-pricing <PATH>`:

- 继续导入完整快照，作为新的 base 并清除已有 overlay。
- 复用同一个 activation/recompute 服务，不保留第二套写入逻辑。

## 7. Embedded Catalog Upgrade

嵌入目录版本从 `static-v1` 升为 `static-v2`：

- 没有用户 snapshot/overlay 且 meta 指向旧 `static-*` 时，bootstrap 只执行一次
  embedded 重算并更新版本，使历史 GPT-5.6 事件得到正确价格。
- 完整用户 snapshot 保持 pinned，不被升级覆盖。
- overlay 基于旧 embedded 时保持 pinned，`catalog status` 提示 rebase；重新 apply 或 reset
  后才使用新 embedded，避免升级时静默改变用户覆盖语义。

该检查以目录版本/meta 为依据，不硬编码 GPT-5.6 ID，因此以后发布新的 embedded 目录仍可复用。

## 8. Cost Calculation

`compute_cost_with` 先解析模型，再按单条事件选择 rate tier：

```text
prompt_tokens = input + cache_read + cache_creation
rate = highest tier where prompt_tokens > prompt_tokens_above
cost = each token channel * selected channel rate
```

- 272,000 本身仍使用 short；272,001 起使用 long。
- reasoning policy 保持现有 included/separate 语义，并由选中 tier 的输出/显式 reasoning
  费率驱动。
- `pricing_rate` 增加 `model_id`、`tier`、`prompt_tokens`、阈值及实际通道费率。
- `cost_without_cache_usd` 在同一选中 tier 下把所有输入通道按该 tier 的 uncached input
  费率计算。
- bucket 只汇总事件成本。若同桶包含 short/long 两档，现有 rollup 应输出 mixed rate，
  不重新套用阈值。

## 9. Validation And Errors

配置入口统一返回 `LlmusageError::ConfigInvalid`/`Parse`，CLI 补充路径上下文。

必须拒绝：

- schema/kind 不匹配；
- 空/重复/非法模型 ID、source 或 matcher；
- 同来源 matcher 同分歧义；
- NaN、infinite、负费率；
- 非递增/重复/零阈值；
- overlay 删除不存在的模型；
- activation meta 指向缺失文件；
- 声明 version 与已保存内容冲突。

已选择目录加载失败继续 fail closed，不回退内置目录。

## 10. Rollback And Failure Shape

- 写文件使用唯一 digest 名，先完成校验和落盘，再开始重算。
- active meta 只在 bucket reconcile 的最终事务切换；失败时旧目录仍是下次启动的选择。
- 现有分页重算可能在失败前留下部分事件使用候选 pricing_source；重试 apply/reset 会幂等修复。
  不为本任务把大库重算改为单超长事务。
- 未被 meta 引用的 content-addressed 文件可保留用于审计；`uninstall --purge` 仍统一清理。

## 11. Documentation Surface

更新：

- `README.md` / `README.zh-CN.md`
- `docs/reference/cli.md` / `docs/zh/reference/cli.md`
- `docs/architecture/index.md` / 中文镜像
- `docs/safety/index.md` / 中文镜像
- `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`

文档说明 v2 示例、优先级、命令、无远程拉取、重算影响、reset 与 legacy 快照兼容。

## 12. Rejected Alternatives

### Maintainer-Only Embedded JSON

改动最小，但用户仍必须等二进制发版，不满足已确认的双层范围。

### Auto-Read A Mutable Well-Known File On Every Run

文件变化后已落库成本会与当前目录不一致，也无法提供原子激活和清晰回滚，拒绝。

### Deep Field Merge

覆盖文件较短，但继承关系难审计，容易组合出从未验证过的费率，拒绝。

### Remote Auto-Update

破坏本地优先和可复现性，也扩大供应链风险，本任务明确不做。

### Apply Threshold To Bucket Totals

官方阈值按请求；桶总量会把多个短请求误算成长请求，拒绝。
