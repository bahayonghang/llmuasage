# Dashboard

`llmusage serve` 会启动本地浏览器 Dashboard 和 JSON API。

```powershell
llmusage serve
```

默认从 `37421` 开始探测本地端口，只绑定 `127.0.0.1`，打印 URL，并尝试打开默认浏览器。

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
6. 数据过旧时使用 sync/export 动作或 diagnostics。

## 筛选器

Dashboard 筛选器映射到 Rust 查询层共享的 `QueryFilter`。

| 筛选 | 含义 |
| --- | --- |
| `source` | `codex`、`claude`、`opencode` 或 `gemini`（`antigravity` 输入别名会映射到 `gemini`） |
| `model` | 标准化事件中的精确模型名 |
| `since` / `until` | Dashboard 查询日期范围 |
| `window` | day/week/month/all 等快速窗口 |
| `timezone` | `UTC`、`local` 或 `+08:00` 这样的固定偏移 |

URL 会保留筛选，刷新页面或复制本地 URL 时仍保持同一视图。

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

## 降级状态

Dashboard 必须显式展示能力缺口，不能把缺失数据伪装成 0。

常见状态：

- `no_data`：当前筛选没有匹配事实。
- `degraded`：行为查询超时或失败，但核心 Dashboard 数据仍已加载。
- `insufficient_models`：模型比较至少需要两个模型候选。
- `low_sample`：可以比较，但样本太少，不能给强结论。
- 来源能力限制：Gemini/Antigravity 和 OpenCode 在源日志不暴露工具级证据时，会退化为保守 turn facts。

Activity、Tools、Optimize、Compare 降级时，核心 `/api/dashboard` 数据仍应保持可响应。

## JSON 导出与静态导出

live Dashboard 可以导出当前 JSON 快照。离线 HTML bundle 使用：

```powershell
llmusage export html --out .\llmusage-report
```

## Sync jobs

live 模式可以启动、轮询、取消进程内 sync job。Job 与 CLI sync 共用同一把本地 worker lock，避免 CLI、hook、Dashboard worker 并发写入。

## 文档截图 fixture

维护文档截图时，用 dev-only 示例生成脱敏数据服务，避免使用真实用户数据：

```powershell
cargo run --features testing --example docs_dashboard_serve -- --port 37421
```

然后以 `1440×1100` 捕获 `http://127.0.0.1:37421`，输出到 `docs/public/screenshots/web-dashboard-overview.png`。
