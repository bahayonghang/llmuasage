# Dashboard

`llmusage serve` 会启动本地浏览器 Dashboard 和 JSON API。

```powershell
llmusage serve
```

默认从 `37421` 开始探测本地端口，只绑定 `127.0.0.1`，打印 URL，并尝试打开默认浏览器。

绑定端口前，`serve` 会检查 parser-backed 来源是否仍使用旧版 token 统计口径，并按
registry 顺序逐源安全重建。存在源文件缺失风险的来源会保持原状并输出告警，因此旧历史
仍可读取，Dashboard 也会继续启动；意外的 parser、SQLite 或提交错误则会终止启动。
自动路径永远不会启用 `--allow-lossy-rebuild`，也不会重建 parserless Antigravity。

需要固定 URL 时指定端口：

```powershell
llmusage serve --port 37421
```

![llmusage 本地 Web Dashboard 概览](/screenshots/web-dashboard-overview.png)

<small>截图来自 `llmusage serve` 启动的脱敏本地 fixture，不是真实用户数据。</small>

## 首屏工作流

首屏按任务组织：

1. 确认当前时间/来源/模型筛选。
2. 看 KPI 条：总 token、成本、cache 和 bucket 数。
3. 用趋势图判断 day/week/month/all 的变化。
4. 对比 project、model、source、cost 排行。
5. 查看行为面板：Activity、Tools、Optimize、Compare。
6. 用 Cost Explorer 回答临时的本地切片分析问题。
7. 数据过旧时使用 sync/export 动作或 diagnostics。

## 筛选器

Dashboard 筛选器映射到 Rust 查询层共享的 `QueryFilter`。

| 筛选 | 含义 |
| --- | --- |
| `source` | `codex`、`claude`、`opencode` 或 `antigravity` |
| `model` | 标准化事件中的精确模型名 |
| `since` / `until` | Dashboard 查询日期范围 |
| `window` | day/week/month/all 等快速窗口 |
| `timezone` | `UTC`、`local` 或 `+08:00` 这样的固定偏移；`local` 表示本机当前固定本地偏移，不是 IANA/DST 感知时区 |

URL 会保留筛选，刷新页面或复制本地 URL 时仍保持同一视图。

Cost Explorer 会在共享筛选之上追加自己的查询控件：

| 控件 | 可选值 |
| --- | --- |
| `granularity` | `total`、`day`、`week` 或 `month` |
| `metric` | `attributed_cost_usd`、`calls`、`turns`、`sessions` 或 `total_tokens` |
| `group_by` | `source`、`model`、`project`、`session`、`tool`、`tool_kind`、`is_tool` 或 `token_type` |
| `limit` / `include_other` | Top N 行，可选择把其余行合并成 `Other` |
| `session_id`、`tool_name`、`tool_kind`、`is_tool`、`token_type` | Explorer 专用筛选 |

## 页面区块

### KPI 与趋势

KPI 条和趋势图来自 `Dashboard::snapshot(&QueryFilter)`。live Dashboard 优先调用 `/api/dashboard`，用一个本地数据库快照构造 overview、trends、rankings、health 和 diagnostics。

### 排行

四类排行回答不同问题：

- Models：哪些模型名贡献主要用量和成本。
- Sources：哪些本地 CLI 产生了数据。
- Projects：哪些本地仓库或目录最活跃。
- Costs：估算成本集中在哪里。

### 行为分析

行为面板读取 sync 阶段生成的 `usage_turn` 和 `usage_tool_call`，不会在浏览器里解析 raw transcript。

| 面板 | 作用 |
| --- | --- |
| Activity | coding、debugging、exploration、testing、planning 等 turn 类别 |
| Tools | read、edit、search、bash、MCP、agent 等工具/动作组合 |
| Optimize | 重复读取、Read/Edit 比过低等只读建议 |
| Compare | 两个模型之间的方向性比较，并显示样本量提醒 |

Optimize 只给建议，绝不删除、移动、归档、重写或清理文件。

### Cost Explorer

Cost Explorer workbench 是新增面板，不替换固定 Dashboard 区块。它用于回答这类问题：

- “今天某类工具调用按 session 分组花了多少？”
- “哪些工具类型贡献了最多归因成本？”
- “按来源切分时，input/cache/output token 组件如何分布？”

浏览器会用当前控件请求 `/api/explorer`。响应已经包含聚合后的 `totals`、排行 `rows` 和时间 `series`；前端只渲染该 payload，不抓取或透视 raw transcript 行。工具相关视图使用 query-time attribution：同一个带成本 turn 中的多个工具按 sibling tool call 分摊成本；无工具但有成本的 assistant turn 在包含非工具时会显示为 `(non-tool)`。

## 降级状态

Dashboard 必须显式展示能力缺口，不能把缺失数据伪装成 0。

常见状态：

- `no_data`：当前筛选没有匹配事实。
- `degraded`：行为查询超时或失败，但核心 Dashboard 数据仍已加载。
- `insufficient_models`：模型比较至少需要两个模型候选。
- `low_sample`：可以比较，但样本太少，不能给强结论。
- `unsupported`：所选 Explorer 指标、维度或筛选组合没有明确语义。
- 来源能力限制：Antigravity 和 OpenCode 在源日志不暴露工具级证据时，会退化为保守 turn facts。

Activity、Tools、Optimize、Explorer、Compare 降级时，核心 `/api/dashboard` 数据仍应保持可响应。

## JSON 导出与静态导出

live Dashboard 可以导出当前 JSON 快照，其中包含当前已加载的 Explorer 结果。离线 HTML bundle 使用：

```powershell
llmusage export html --out .\llmusage-report
```

静态 bundle 的 `snapshot.json` 会包含默认 Explorer payload，并带有同一套 Explorer 渲染资产。Snapshot 模式会禁用 live Explorer 控件，因为它读取捕获的 JSON，而不是访问 `/api/explorer`。

## Sync jobs

live 模式可以启动、轮询、取消进程内 sync job。Job 与 CLI sync 共用同一把本地 worker lock，避免 CLI、hook、Dashboard worker 并发写入。

## 文档截图 fixture

维护文档截图时，用 dev-only 示例生成脱敏数据服务，避免使用真实用户数据：

```powershell
cargo run --features testing --example docs_dashboard_serve -- --port 37421
```

然后以 `1440×1100` 捕获 `http://127.0.0.1:37421`，输出到 `docs/public/screenshots/web-dashboard-overview.png`。
