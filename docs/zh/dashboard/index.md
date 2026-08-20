# Dashboard

`llmusage serve` 会启动本地浏览器看板和 JSON API。

```powershell
llmusage serve
```

默认从 `37421` 开始探测本地端口，绑定 `127.0.0.1`，打印 URL，并尝试打开默认浏览器。

绑定端口前，`serve` 会检查 parser-backed 来源是否仍使用旧版 token 统计口径，并按
registry 顺序逐源安全重建。存在源文件缺失风险的来源会保持原状并输出告警，因此旧历史
仍可读取，看板也会继续启动；意外的 parser、SQLite 或提交错误则会终止启动。
自动路径永远不会启用 `--allow-lossy-rebuild`，也不会重建 parserless Antigravity。

需要固定 URL 时指定端口：

```powershell
llmusage serve --port 37421
```

## 远程或 SSH 访问

远程服务器需要显式开启监听，并关闭浏览器启动：

```powershell
llmusage serve --public --no-open --port 37421
```

`--public` 会绑定 `0.0.0.0`；请从可访问服务器的机器打开 `http://<server-host-or-ip>:37421`。编译期路由白名单只包含页面骨架、静态资源、`/api/dashboard` 和 `/api/health`。公开看板只返回聚合总览、趋势、模型、来源和成本；项目名称、原始日志、诊断信息、游标明细、任务状态、行为明细、用量分析和写路由都不可用。公开看板请求也会忽略 `project` 和 `project_hash` 筛选，避免通过聚合结果间接探测特定项目。

这个精简 public surface 仍不提供认证或 TLS，并仍会显示用量总量和模型/来源名称，因此仍需防火墙或带认证的反向代理。远程使用全部本地 Dashboard 功能时，不要使用 `--public`，应保留默认 loopback 监听并使用下面的 SSH 隧道。

SSH 也可以作为用量导入的数据通道。见 [CLI 参考](../reference/cli.md#llmusage-remote) 中的 `llmusage remote add` / `llmusage sync`。这条拉取路径与 Dashboard 隧道是分开的。

私有 SSH 场景不要传入 `--public`，再从客户端转发本地监听端口：

```powershell
ssh -L 37421:127.0.0.1:37421 <user>@<server>
```

SSH 会话会自动跳过浏览器启动。

![llmusage 本地 Web Dashboard 概览](/screenshots/web-dashboard-overview.png)

<small>截图来自 `llmusage serve` 启动的脱敏本地 fixture，不是真实用户数据。</small>

## 首屏工作流

首屏按任务组织：

1. 确认当前时间/来源/模型筛选。
2. 看六张摘要卡：会话数、请求数、Token 用量、估算成本、活跃天数和缓存读取占比。
3. 查看每日活跃度、每周活跃时段和每日 Token 用量构成，再用短时趋势观察近 24 小时细节。
4. 对比高用量会话与项目、模型、来源和成本排行。
5. 查看行为面板：活动类型、工具使用、优化建议和模型对比。
6. 用“用量分析”回答临时的本地多维分析问题。
7. 在事件日志中分页查看事件细节；数据过旧时使用同步、CSV 导出或诊断信息。

屏幕宽度不超过 `720px` 时，数据状态卡会收敛为首屏内的紧凑折叠摘要；展开后可查看同步游标和最近失败。宽屏仍显示完整状态卡；看板不再展示集成安装状态。

## 筛选器

看板筛选器映射到 Rust 查询层共享的 `QueryFilter`。

| 筛选 | 含义 |
| --- | --- |
| `source` | `codex`、`claude`、`opencode`、`antigravity`、`kimi_code`、`pi` 或 `grok` |
| `model` | 标准化事件中的精确模型名 |
| `since` / `until` | 看板查询日期范围 |
| `window` | day/week/month/all 等快速窗口 |
| `timezone` | `UTC`、`local` 或 `+08:00` 这样的固定偏移；`local` 表示本机当前固定本地偏移，不是 IANA/DST 感知时区 |

URL 会保留筛选，刷新页面或复制本地 URL 时仍保持同一视图。

Antigravity CLI 会话由已注册解析器导入。hook 时代的 Antigravity 记录仍可在报表和看板筛选中查看。这些记录没有文件归属时，重建会被拒绝。IDE 侧 `conversations/*.pb` 仍为计划项。

用量分析会在共享筛选之上追加自己的查询控件：

| 控件 | 可选值 |
| --- | --- |
| `granularity` | `total`、`day`、`week` 或 `month` |
| `metric` | `attributed_cost_usd`、`calls`、`turns`、`sessions` 或 `total_tokens` |
| `group_by` | `source`、`model`、`project`、`session`、`tool`、`tool_kind`、`is_tool` 或 `token_type` |
| `limit` / `include_other` | 最多显示的结果数，可选择把其余结果合并成“其他” |
| `session_id`、`tool_name`、`tool_kind`、`is_tool`、`token_type` | 用量分析专用筛选 |

## 页面区块

### 摘要、每日活跃度与趋势

六张摘要卡会随当前筛选显示会话数、请求数、Token 用量、估算成本、活跃天数和缓存读取占比；高亮的 Token 用量卡还会标出用量最高的来源。每日活跃度可以切换 Token 用量 / 请求数强度，支持键盘焦点和提示信息；点击日期会把全局筛选下钻到当天，再次点击则恢复之前的范围。近 1 天会隐藏年历，把整行留给每周活跃时段；大约一个月及以内改为带日期的日条；全部范围保留按周排列的年历并拉满面板宽度。

每日堆叠图分开展示输入、缓存读取、缓存写入和输出 Token，并在提示信息中显示当日估算成本。近 24 小时范围会明确显示空态，由现有短时趋势提供更细粒度的观察。实时模式下，这些面板与活动类型、工具使用、优化建议、用量分析和模型对比一样进入“最新请求优先”的次级加载生命周期，因此旧响应不能覆盖新筛选。

静态 HTML 导出会在 `snapshot.json` 中保存精简摘要、最多 366 天的热力图和每日序列。缺少这些键的旧快照会显示空态，不会导致页面报错。实时看板首次加载 `/api/dashboard`；范围切换、自动刷新和同步完成后的刷新使用 `scope=interactive` 并独立加载次级面板，因此慢查询或降级面板不会阻塞首屏。

每周活跃时段会按浏览器 IANA 时区把 30 分钟桶折叠成周一优先的 `7 x 24` 网格；宽屏下它与每日活跃度并排，近 1 天则独占整行，中窄屏自动纵向排列。高用量会话支持由服务端按 Token 用量、活跃时长和估算成本排序；点击会话会跳转到事件日志，并由服务端过滤该会话。展开事件行时才按需读取保留的原始 JSON。事件日志仅在实时模式可用，沿用每页 50 条的游标分页。

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
| 活动类型 | 编码、调试、探索、测试、规划等活动类别 |
| 工具使用 | 读取、编辑、搜索、命令行、MCP、子代理等工具或操作组合 |
| 优化建议 | 重复读取、读写比例过低等只读建议 |
| 模型对比 | 两个模型之间的方向性比较，并显示样本量提醒 |

优化建议只提供只读提示，绝不删除、移动、归档、重写或清理文件。

### 用量分析

用量分析是独立工作区，不替换固定看板区块。它用于回答这类问题：

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
- `unsupported`：所选用量分析指标、维度或筛选组合没有明确语义。
- 来源能力限制：Antigravity 历史行和 OpenCode 行在源日志不暴露工具级证据时，会退化为保守 turn facts。

活动类型、工具使用、优化建议、用量分析或模型对比降级时，核心 `/api/dashboard` 数据仍应保持可响应。

## CSV 导出与静态导出

实时看板会把当前已加载的摘要、每日趋势、项目、模型、来源与高用量会话
导出为带 UTF-8 BOM 的 CSV；不可信标签会先做公式注入防护，再按标准 CSV 规则转义。
离线 HTML bundle 使用：

```powershell
llmusage export html --out .\llmusage-report
```

静态包的 `snapshot.json` 会包含摘要卡、每日活跃度、每周活跃时段、高用量会话、每日 Token 序列、默认用量分析数据和对应渲染资产。旧快照缺少新键时仍可安全显示空态。离线快照会禁用实时用量分析控件，并把事件日志标记为仅实时看板可用。

## 同步任务

实时模式可以启动、轮询和取消进程内同步任务。任务与 CLI `sync` 共用同一把本地同步执行锁，避免 CLI 与看板并发写入。

## Live 刷新与 HTTP 传输

- 自动刷新（`30s` 或 `60s`）和 sync 完成后的刷新统一复用 interactive 路径，自定义 `since`/`until` 筛选也不回退 full scope；面板数据未变时不会重复写 DOM。
- 文件系统 diagnostics 在 Web 请求边界缓存 30 秒，并由 `/api/diagnostics` 与各 dashboard scope 共用。sync 完成、失败或取消，以及显式 forget diagnostics，都会立即失效缓存；库 API 的 `Dashboard::diagnostics()` 仍保持 cold read。
- 内嵌 CSS、JavaScript、SVG 资源返回 `Cache-Control: no-cache` 和内容 ETag，浏览器可协商得到无 body 的 `304`，无需引入版本化 URL。文本资源和 JSON 响应支持 gzip/Brotli 协商压缩；API JSON 不增加缓存。

## 文档截图 fixture

维护文档截图时，用 dev-only 示例生成脱敏数据服务，避免使用真实用户数据：

```powershell
cargo run --features testing --example docs_dashboard_serve -- --port 37421
```

然后以 `1440×1100` 捕获 `http://127.0.0.1:37421`，输出到 `docs/public/screenshots/web-dashboard-overview.png`。
