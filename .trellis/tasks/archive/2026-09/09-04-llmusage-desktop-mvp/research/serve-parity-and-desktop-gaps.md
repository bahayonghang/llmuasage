# Serve 面板清单与 Desktop 缺口

对照源：`src/web/shell.rs`、`src/web/assets/load-state.js`、`docs/dashboard/index.md`、`src/web/mod.rs` 路由表、TUI `src/tui/panels/`、`.trellis/spec/llmusage/backend/tui-subscription-contracts.md`。

## 1. `llmusage serve` 用户可见面（选项 C 对齐对象）

### 壳

- 侧栏导航：用量概览、用量趋势、模型分布、来源分布、项目排行、行为分析、用量分析、成本估算、运行状态、事件日志
- 主题 light/dark（`localStorage llmusage:theme`）
- 语言 zh/en（`localStorage llmusage:locale`）
- 顶栏：导出 CSV、自动刷新 off/30s/60s、同步/取消
- 筛选：source、model、range 1d/7d/30d/all/custom、since/until；URL 保留 filter；timezone 默认浏览器 IANA，可走 query；点击项目写入 `project_hash`

### 核心快照（interactive，先绘制）

`Dashboard::interactive_snapshot`：overview、单段 trends、models、sources、hosts、projects、costs、sync_command_center、diagnostics、health summary。

核心超时：2s 标慢、6s 中止。失败不回退到旧 section 扇出。

### 次级面板（并发 2，latest-request-wins）

`SECONDARY_SECTIONS`：`activity`、`tools`、`optimize`、`explorer`、`compare`、`home_overview`、`heatmap`、`trends_daily`、`top_sessions`、`hour_of_week`。

对应 UI：

| 面板 | 行为 |
|---|---|
| 六张 summary 卡 | home_overview：sessions / requests / tokens / cost / active days / cache-read share |
| Daily activity | heatmap，点击日期钻取，再点还原 |
| Weekly activity | hour_of_week，浏览器时区 Monday-first 7×24 |
| Session ranking | top_sessions，可按 tokens/duration/cost 排序；点行打开 logs 并带 session 过滤 |
| Token usage mix | trends_daily，input/cache-read/cache-creation/output/other |
| Activity / Tool usage / Optimize / Compare | 行为事实；support 状态显式展示 |
| Explorer | 独立控件：metric/group_by/granularity/limit/session/tool/token_type/include_other/include_non_tool |
| Costs | 来源×模型估算 |
| Status | insights + 最近失败 + diagnostics |
| Logs | live JS 每页 20 行（`LOGS_PAGE_SIZE`）；Rust `LogsQuery` 默认 50（page_size=0）。Desktop 对齐 live 20。展开行按需读 raw JSON。仅 live |

### 写入

- `POST /api/jobs`：source 取当前 filter；`recent_days` 由 range preset 映射 1/7/30
- `POST /api/jobs/{id}/cancel`
- 与 CLI 共用 `worker_lock`

### 降级状态（必须保留）

`no_data`、`degraded`、`insufficient_models`、`low_sample`、`unsupported`、source-limited facts。核心面板在次级降级时仍可用。

### Serve 有 API、界面没有

- `POST /api/diagnostics/forget`：ccr-ui/库用，vanilla 看板无按钮。
- 独立 section HTTP（`/api/overview` 等）：兼容面，live 默认走 interactive + secondary。

## 2. Serve 有、Desktop 不应照搬

- 侧栏 `127.0.0.1:37421` 与「本地服务在线」
- `--public` / SSH 隧道 / Origin-Host 写保护
- 浏览器 bootstrap watchdog（探测 `/`）
- 静态 `export html` snapshot 模式（`data-mode=snapshot`）
- HTTP ETag / gzip / 10s live fetch cache（IPC 可另做 generation 取消，不搬 HTTP 缓存语义的 URL 形态）

## 3. 独立 Desktop 缺了会站不住的能力（serve 当浏览器页不需要）

| 缺口 | 原因 |
|---|---|
| 未初始化可在应用内 bootstrap | 独立应用不能假定用户先跑过 `llmusage init` |
| `SchemaTooNew` 可见 | 库比 Desktop 新时禁止瞎迁 |
| `LockBusy` / `LockLost` 可见 | CLI `sync`/`serve` 与 Desktop 互斥写 |
| 单实例 | 两个 Desktop 进程会争锁 |
| 用 `root_dir` / `db_path` 替换 HTTP 地址 | 路径 B 无监听端口 |
| CSV 走系统保存对话框 | WebView 里 `a[download]` 不稳定 |
| 筛选/主题/语言持久化到本机配置 | 没有 URL 可分享 |

## 4. TUI 有、serve 没有（选项 C 默认不包含）

- Usage 订阅额度：Claude / Codex / Grok / Kimi，本地凭证只读，5 分钟缓存，`r` 刷新。Web 看板零引用。
- TUI 面板形态：daily/weekly/monthly/blocks/stats 报表，与 serve 的 trends/heatmap 重叠但不是同一 UI。
- Source Sync overlay（`y`），web 已有 sync_command_center。

## 5. 本仓库其它产品岛（默认不进 MVP）

- `codex-tracer.db`：独立库，架构文档禁止并入 `Store`
- `llmusage remote` SSH 导入：无看板 UI
- `catalog apply` / `doctor --refresh-pricing`：无看板 UI
- 托盘 / 开机自启 / 通知 / 签名安装包 / 自动更新：agentsview/TokenTracker 有，serve 没有

## 6. 对选项 C 的结论

选项 C = 重做 serve 的全部可见面板 + 同步生命周期 + 降级语义，改走 Tauri IPC。

还需要补 3 节的桌面自立能力，否则只是无浏览器的 serve。

用户已拍板：订阅额度进 MVP，走 TUI `subscription::fetch_all`，不新写拉取器。

forget、托盘、安装包、remote、catalog、tracer 仍不是 C 的自动范围，需单独拍板。
