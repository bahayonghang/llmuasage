# 对齐 ccusage README 命令与过滤器 surface（父任务）

## Goal

让 `llmusage` 的报表命令与过滤器 surface **严格对齐** ccusage README/参考实现所列、且当前 llmusage
缺失或语义不一致的部分。**决策 = 严格 parity**（见 D0）：接受对已发布 v1.0 默认文本表与 CLI JSON
的破坏性变更，以换取与 ccusage 一致的行为。父任务拥有需求来源、子任务图、跨子任务验收与共享约定；
实现由各子任务独立规划/实现/检查/归档。

参考基线：ccusage checkout `31e084a`（2026-07-20）。

## D0 严格 parity 决策（本轮定调，覆盖此前 D1/D2）

- **默认统一文本表带 Agent 列**：daily/weekly/monthly 每个周期键先出一行聚合（Agent=`All`），
  再列各来源子行（`- <source>`）；session 行本就按来源。→ **改变 v1.0 默认输出**，须更新既有快照。
- **`--by-agent` 降为 JSON-only**：仅给 daily/weekly/monthly 的 JSON 行追加 `agents` 数组；
  session 不受影响。文本表不因该 flag 改变（本就有 Agent 列）。
- **CLI 报表 JSON 采用 ccusage schema + camelCase**：`{ "<period>": [rows], "totals": {…} }`，
  行键 `period/agent/modelsUsed/inputTokens/outputTokens/cacheCreationTokens/cacheReadTokens/
  totalTokens/totalCost/modelBreakdowns[/agents]`。→ **破坏 llmusage 现有 snake_case CLI JSON**。
- **weekly 周键 = 周起始日期**（统一视图周一起，如 `2025-12-29`），非 ISO 周号。
- **Detected 在文本框标题**：`Coding (Agent) CLI Usage Report - <Period>\nDetected: <labels>`；
  JSON 不含 `detected`。
- **删除旧 P7（默认输出逐字节不变）**：与本决策互斥，不再作为验收项（新验收表见文末 P1–P9）。

## 破坏性变更面（迁移清单，父任务收尾核对）

- 默认 `daily/monthly/session` 文本表列与行结构变化 → 更新所有相关渲染快照测试。
- CLI `--json` 键名由 snake_case → camelCase、结构改为 `{<period>:[...],totals}` → 更新 JSON
  断言与任何下游消费者/文档示例。
- CLI 报表 JSON 用**独立 DTO/视图层**产出 ccusage schema，**不改** `TokenTotals` 等内部结构的
  Serialize（避免污染 dashboard/web/export 内部 payload）。内部 payload 契约保持不变。
- `README.md` / `README.zh-CN.md` / `docs/` 命令示例同步更新。

## 需求来源：ccusage README/参考实现 ↔ llmusage 对照

| ccusage surface | llmusage 现状 | 结论 |
| --- | --- | --- |
| `daily`(默认) / `monthly` / `session` / `blocks` | 已有（合并单行，无 Agent 列） | **改造为 Agent 列统一表**（C1） |
| `weekly` | 缺 | **补**（C2，周起始日期键） |
| 统一文本表 Agent 列 + Detected 标题 | 无 | **补**（C1） |
| `--by-agent`（JSON 加 agents，仅 d/w/m） | 无 | **补**（C1；weekly 段随 C2） |
| `--sections a,b,c`（当前段必含/去重/固定序/扁平 JSON+命令段 totals） | 缺 | **补**（C4） |
| `<source> <period>` 聚焦（能力矩阵各异，无 Agent 列） | 仅 `--source` flag | **补**（C5，能力感知） |
| `--no-cost`（列省略 + JSON cost strip） | 缺 | **补**（C3） |
| `--since/--until` 接受 `YYYY-MM-DD` | 仅 `YYYYMMDD` | **补**（C6，保留兼容） |
| `--json` `--timezone` `--breakdown` `--compact` `--instances` `--project` | 已有 | 保持（并入新表/JSON） |
| `--all`（统一=默认，仅兼容占位） | `--all`=全历史 | 见 D1' |
| `--mode` `--speed` `--offline` `--pi-path` `ccusage.json` overrides | ccusage 内部细节 | **Out of Scope** |
| 新增来源（Gemini/Kimi/Qwen/Copilot…） | 仅 4 来源 | **Out of Scope** |

- **D1'（`--all`）**：ccusage 中统一视图即默认，`--all` 仅为兼容占位。llmusage 现 `--all`=全历史。
  保留 llmusage 现语义；统一/组合能力由默认表 + `--sections` 承担。不把 `--all` 改为 ccusage 占位义。

## 共享约定

- `--no-cost` 与 `YYYY-MM-DD` 日期解析加在共享 `ReportCommonArgs`（`src/commands/report_args.rs`），
  一次生效于所有报表命令。
- 统一报表走**单一 period-kind 参数化**路径（对齐 ccusage `adapter/all`），daily/weekly/monthly/
  session 复用同一渲染器与 JSON DTO。
- 聚焦视图（`<source> <period>`）**不含 Agent 列**（对齐 ccusage "focused views remove the
  comparison layer"）。

## 子任务图（build order：C1 → C2 → C3 → C4；C6 任意；C5 在 C2 后）

| 子任务 | 交付物 | 依赖 |
| --- | --- | --- |
| C1 `daily-by-agent-report`(unified-agent-report) | 统一 Agent 列表(daily/monthly/session) + camelCase JSON + `--by-agent` JSON agents + Detected 标题 | 无（基础） |
| C2 `weekly-report` | `weekly`(周起始日期键)，接入 C1 统一模型（Agent 列/JSON/by-agent 一致） | **C1** |
| C3 `no-cost-flag` | `--no-cost`：文本省略成本列 + JSON cost strip，覆盖所有报表 | **C1**（渲染器/JSON 就绪后） |
| C4 `sections-composite-report` | `--sections`（当前段必含/去重/固定序/当前优先；扁平 JSON+命令段 totals） | **C1、C2** |
| C5 `source-subcommands` | 聚焦 `<source> <period>`，按真实能力矩阵，无 Agent 列 | **C2**（weekly 组合） |
| C6 `date-filter-format` | `--since/--until` 接受 `YYYY-MM-DD`，保留 `YYYYMMDD` | 无 |

依赖写进各子任务 `prd.md`/`implement.md`，不靠树结构表达。**仅 C1 需先完成 design/implement 并
start；其余子任务在各自开工前补齐 design/implement 再 start**（复杂任务须 prd+design+implement，
C6 可 PRD-only）。

## 跨子任务验收

- [ ] P1：`llmusage weekly` 接入统一模型，周键为周起始日期，与 daily/monthly 同构。
- [ ] P2：daily/weekly/monthly **默认文本表**含 Agent 列（`All` + 各来源子行）与 `Detected:` 标题；
      session 文本按来源。
- [ ] P3：CLI `--json` 为 `{ "<period>": [rows], "totals": {…} }`、camelCase；`--by-agent` 给
      daily/weekly/monthly 行加 `agents` 数组，session 不变。
- [ ] P4：`--no-cost` 在所有报表命令下省略成本列并从 JSON strip 成本字段，不影响 token 列。
- [ ] P5：`--sections`（含当前段、去重、固定周期序、当前优先）文本逐段输出；JSON 扁平且带命令段
      `totals`。
- [ ] P6：`<source> <period>` 聚焦按真实能力矩阵可用（Claude 含 blocks；Codex=d/m/s；OpenCode=+weekly），
      聚焦视图无 Agent 列。
- [ ] P7：`--since 2026-04-25` 与 `20260425` 等价，对所有报表命令生效。
- [ ] P8：内部 payload（dashboard/web/export/TUI）契约不受 CLI-JSON 改造影响。
- [ ] P9：`just ci` 全绿（fmt/clippy/`cargo test --test-threads=1`/doc/node 检查/docs 构建）；
      `README.md`+`README.zh-CN.md`+`docs/` 命令页更新。

## Out Of Scope

- `statusline` 任何改动。
- 新增来源解析器（Gemini CLI / Kimi / Qwen / Copilot / Amp / Droid …）。
- 免 `sync` 即时读取本地 JSONL 的数据流改造。
- ccusage 内部标志：`--mode` / `--speed` / `--offline` / `--pi-path` / `ccusage.json` 定价覆盖。
- 改变 token 统计口径、成本计算、SQLite schema 或同步行为；改变内部（非 CLI）JSON payload 键名。

## 集成评审（父任务收尾）

- 全部子任务归档后跑 `just ci` 全量门禁 + 手动 smoke（每个新命令/flag 各一次），核对 P1–P9 与
  破坏性变更迁移清单。
- 在 `.trellis/spec/llmusage/backend/` 记录报表 CLI surface 契约（命令清单、统一 Agent 模型、
  camelCase JSON schema、by-agent/sections/no-cost 语义、周键、日期格式、`--all` 决策、聚焦能力矩阵）。
- 更新 `README.md` / `README.zh-CN.md` / `docs/` 命令用法与示例。
