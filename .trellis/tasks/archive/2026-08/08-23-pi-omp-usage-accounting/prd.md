# 补全 Pi 与 Oh My Pi 用量统计

## Goal

把 Pi / Oh My Pi 从「一个合并源、只有 token」升级为：两个可分辨的源、带 provider 与项目维度、成本非零、并进入行为看板。

## 测量口径（必读）

本文档与四个子任务引用的所有条数、金额都由同一个只读脚本产出：

```bash
python .trellis/tasks/08-23-pi-omp-usage-accounting/research/scan_pi_source.py
```

口径定义写在脚本头部，两条关键点：

- **递归枚举** `<root>/**/*.jsonl`。Oh My Pi 会把命名子会话写在项目目录再下一层
  （`<project>/<run-dir>/<Name>.jsonl`），而 `llmusage` 的发现用 `WalkDir` 递归
  （`src/parsers/source_files.rs:258`），所以一层 glob 会少算。本机 28 个文件里
  9 个属于这种嵌套布局。
- 「usage 记录」= `message.usage` 为对象的记录，与 `src/parsers/pi.rs` 产出事件的判定一致。

真源在持续写入，**任何验收都必须在验收当时重跑该脚本取值**，不要沿用本文档的快照数字。

## 背景与现状证据

Pi / Oh My Pi 已经接入（`SourceKind::Pi`，`src/parsers/pi.rs`），发现路径合并
`~/.pi/agent/sessions` 与 `~/.omp/agent/sessions`（`src/parsers/source_files.rs:167`），
两根产出的事件都写 `source = pi`。

数据库侧（`~/.llmusage/llmusage.db`，最近一次 pi 同步 2026-08-22T16:29Z）：

- `source='pi'`：423 条事件、33,137,033 token、`pricing_status='unpriced'` 423/423、
  `cost_with_cache_usd` 合计 0.00、`provider_label` 空 423/423、`project_hash` 空 423/423。
- `usage_turn` 与 `usage_tool_call` 中 `source='pi'` 均为 0 行。
- `source_file` 中 `source='pi'` 有 19 行、全部 `state='live'`，全部来自 `.omp`。
  19 与真源 28 的差额是「上次同步之后新写入的 9 个嵌套文件尚未同步」，
  不是发现遗漏：这 9 个文件的 mtime 都晚于 `last_seen_at`。

真源侧（扫描时间 2026-08-23，28 个文件、1198 条 usage 记录）：

| 字段 | 覆盖 | 说明 |
| --- | --- | --- |
| `message.usage.cost{input,output,cacheRead,cacheWrite,total}` | 1198/1198 | 118 条 `total > 0`，合计 $0.441788 |
| `message.provider` | 1198/1198 | openrouter 989、deepseek 115、xai-oauth 86、openai-codex 8 |
| 会话头 `cwd` | 28/28 | 嵌套子会话文件同样有完整会话头 |
| `message.content[].type == "toolCall"` | 1449 个块 | `arguments` **全部是 JSON 对象**（1449/1449，字符串 0） |
| `message.retryRecovery` | 3 | `{kind,status,attempt,recovery,supersededBy}` |
| `childUsage` / `aggregateUsage` | 0 | 真源不写子会话归集用量，因此子会话不存在父子重复计数 |

按 provider 的成本覆盖：openai-codex 6/8 = $0.3756、deepseek 112/115 = $0.0661、
xai-oauth 0/86、openrouter 0/989。

已验证**不需要**改动的点：

- 真源写的键是 `reasoningTokens`（182 条），`src/parsers/pi.rs:479` 键名正确。
  参考实现 tokscale 读的 `reasoning` 是另一种拼写。
- 1198/1198 记录满足 `totalTokens == input + output + cacheRead + cacheWrite`，
  且 `reasoningTokens <= output` 恒成立。当前「权威 total 优先、reasoning 独立
  不计入 total」的算法与真源一致，`ReasoningPolicy::IncludedInOutput` 默认值也一致。
- 写入端已实现路径级行为事实清理（`src/store/sync_writer.rs:656` 的
  `reset_behavior_facts_batch_tx` 由 `reset_path_hashes` 驱动），行为子任务不需要另造机制。

## 参考实现对照

- **ccusage**（`ref/repo/ccusage/rust/adapters/pi/`）：默认只读 `~/.pi/agent/sessions`；
  `.omp` 通过配置 `pi.stores[]` 作为 named store 接入，报表里 agent 显示为 `omp`、
  模型名前缀 `[omp] `；读 `usage.cost.total` 作为 display cost（`auto` 模式优先）；
  project 取 `sessions` 之后一段目录名。重叠路径判定是**双向**的
  （`left == right || left.starts_with(right) || right.starts_with(left)`，
  `ref/repo/ccusage/rust/crates/ccusage-adapter-all/src/loader.rs:511`）。
- **tokscale**（`ref/repo/tokscale/crates/tokscale-core/src/sessions/pi.rs`）：
  `.omp` 挂在 `ClientId::Pi` 的第二个扫描根，与当前 llmusage 相同；读 `message.provider`，
  缺失或空串时按模型名推断；从会话头取 `cwd` 做 workspace 标签。

两个参考实现都不按根拆分独立源，本任务按用户决定采用拆分方案，理由记在子任务 design。

## Requirements

- **R1** `.omp` 根产出独立源 `omp`，`.pi` 根保留源 `pi`；两者各自持有游标、
  token accounting 版本与报表行。
- **R2** 升级不得产生 token 双计：同一条真源记录不得同时以 `pi:` 与 `omp:` 前缀入库。
  该要求覆盖三条路径：默认 `sync`、限定源 `sync --source omp`、远端主机导入。
- **R3** 每条 Pi/OMP 事件写入 `provider_label`，取自 `message.provider`。
- **R4** 每条 Pi/OMP 事件写入项目维度（`project_hash`/`project_label`），取自会话头 `cwd`，
  回落到「root 下第一段目录名」解码，且对嵌套子会话布局成立。
- **R5** 真源自带成本可用时成本列非零，并可与目录定价结果区分；事件行与聚合桶
  （`usage_bucket_30m`）都不得被目录重算清零。
- **R6** Pi/OMP 进入行为看板：产出 `usage_turn` 与 `usage_tool_call`。
- **R7** 保持既有隐私边界，不放宽也不单独收紧：
  - 不落库转录文本；
  - 源文件路径只以哈希落库（`source_path_hash` / `path_hash`）；
  - `usage_tool_call.safe_preview` 沿用 `src/parsers/behavior.rs` 的既有实现与 120
    字符上限。该预览会包含工具参数里的 `file_path` / `path` / `command` 原文片段，
    这是 claude / codex / opencode 已有的行为（库中 39,202 条预览里 34,946 条含路径
    分隔符），本任务与之保持一致。若要改这条边界，属于跨源的独立决定，不在本任务内。
- **R8** 历史回填的触发方式必须显式写明：子任务 2/3/4 的记录级改动不会被普通增量
  `sync` 回填（游标会跳过未变化文件，`src/parsers/pi.rs:134`、`:146`），
  必须由 `sync --rebuild --source omp` 完成。该口径与 ADR 0010「历史归属需要
  `sync --rebuild`」的既有先例一致。

## 非目标

- 不给 `pricing/static-v2.json` 增加 pi/omp 的模型定价行（真源路由到 openrouter 等
  任意模型，逐条补目录不可收敛）。用户自备 snapshot/overlay 目录可以包含 `omp` 行，
  本任务不禁止，见子任务 3 的优先级设计。
- 不接入 Pi 家族其他衍生客户端（Senpi、Kimchi、Prime Agent）。
- 不使用 `contextSnapshot` 做上下文压力面板。
- 不引入 `duration`/`ttft` 延迟指标。
- 不改 `safe_tool_preview` 的脱敏策略（见 R7）。

## 子任务地图

| 顺序 | 子任务 | 覆盖 | 依赖 |
| --- | --- | --- | --- |
| 1 | `08-23-omp-source-split` | R1 R2 R8 | 无 |
| 2 | `08-23-pi-event-dimensions` | R3 R4 | 子任务 1 |
| 3 | `08-23-pi-source-cost` | R5 | 子任务 1 |
| 4 | `08-23-pi-behavior-signals` | R6 | 子任务 1、2 |

子任务 2、3 相互独立，都改 `src/parsers/pi.rs` 的事件构造点，按顺序做以避免冲突。
R7 是四个子任务共同的约束；R8 由子任务 1 建立机制、其余子任务在自己的验收里使用。

集成、跨子任务回滚与远端主机范围写在本任务的 `design.md` 与 `implement.md`。

## Acceptance Criteria

编号后缀标注覆盖的 requirement。

- [ ] **AC1**（R1）四个子任务全部归档，`just ci` 通过。
- [ ] **AC2**（R1）`llmusage sync` 后 `usage_event` 中存在 `source='omp'` 的行；
      本机无 `.pi` 数据，故 `source='pi'` 为 0 行。
- [ ] **AC3**（R2）迁移保真：以升级 sync 之前导出的 `(source_path_hash, event_at,
      model, total_tokens)` 集合为基线，升级后 `omp` 行必须覆盖该基线的每一条身份，
      差集为空；新增行只允许来自基线之后的新记录。
- [ ] **AC4**（R2）不存在同一条真源记录同时以 `pi:` 与 `omp:` 前缀入库的行。
- [ ] **AC5**（R2）`sync --source omp` 在存量 `pi` 行未迁移时不会写入 `omp` 行
      （要么先迁移，要么带明确信息拒绝）。
- [ ] **AC6**（R2）远端主机路径有明确结论：要么实现 host 级迁移，要么在文档与
      命令输出里写明必须执行的手动步骤，并有测试覆盖所选方案。
- [ ] **AC7**（R3）`source='omp'` 行的 `provider_label` 非空比例 100%，
      去重值与当次扫描的 provider 集合一致。
- [ ] **AC8**（R4）`source='omp'` 行的 `project_hash` 非空比例 100%，
      且嵌套子会话文件产生的行与其顶层项目同属一个 `project_hash`。
- [ ] **AC9**（R5）`source='omp'` 的 `SUM(cost_with_cache_usd)` 与当次扫描的
      `cost_total` 差值绝对值 < $0.000001；`usage_bucket_30m` 中 `source='omp'`
      的成本合计与事件侧一致（同一阈值）。
- [ ] **AC10**（R6）`usage_turn` 与 `usage_tool_call` 中存在 `source='omp'` 的行，
      且 `usage_tool_call` 行数等于当次扫描的 `tool_call_blocks`。
- [ ] **AC11**（R7）负向断言存在：`usage_event` 与 `usage_turn` / `usage_tool_call`
      中不出现原始会话文件路径字符串（只有哈希）。
- [ ] **AC12**（R8）四个子任务各自的本机验证都通过 `sync --rebuild --source omp`
      执行，且文档写明该步骤。
- [ ] **AC13**（R1）`README.md`、`README.zh-CN.md`、`docs/index.md`、`docs/zh/index.md`、
      `docs/reference/cli.md`、`docs/dashboard/index.md` 与 `docs/guide/first-sync.md`
      列出 `omp`；`llmusage --help` 的 `--source` 说明与 `diagnostics` 的错误提示也列出 `omp`。
