# llmusage

[English](./README.md)

本地优先的 Rust CLI。

目标很直接：用 hook 和本地 SQLite 复现多 CLI 用量采集，不上传、不登录、不连云端 API。

感谢 [vibeusage](https://github.com/victorGPT/vibeusage) 提供思路。`llmusage` 在它的基础上用 Rust 做了重构和改进，并把本地优先这条边界收得更紧。

当前 0.5.1 覆盖：

- Codex `config.toml notify`
- Claude `Stop` / `SessionEnd` hooks
- OpenCode `session.updated` plugin event
- Gemini `SessionEnd` hooks 与 `~/.gemini/tmp/*/chats/session-*.json` 解析

核心真源：

- 配置目录：`~/.llmusage/`
- 数据库：`~/.llmusage/llmusage.db`
- hook 包装器：`~/.llmusage/bin/llmusage-hook.cmd`、`~/.llmusage/bin/llmusage-hook.sh`

命令：

报表优先命令（只读本地 SQLite；如果数据看起来过旧，先运行 `llmusage sync`）：

- `llmusage` / `llmusage daily`
- `llmusage monthly`
- `llmusage session`
- `llmusage blocks`
- `llmusage statusline`

`llmusage` / `llmusage daily` 默认按所选时区显示过去 7 个自然日（包含今天）；human 输出改为一张聚合的 ccusage 风格表，列为 `Date / Models / Input / Output / Cache Create / Cache Read / Total Tokens / Cost (USD)`，token 使用完整逗号分隔数字，模型以多行列表展示；`NO_COLOR=1` 会禁用 ANSI 样式。JSON 输出保持聚合 snake_case，并包含 `cache_creation_tokens`。需要完整 daily 历史时使用 `--all`，需要指定范围时使用 `--since YYYYMMDD` / `--until YYYYMMDD`，可用 `--source` 过滤单个本地来源，或用 `--breakdown` 输出按来源/模型拆分的明细。

常用报表参数包括 `--since YYYYMMDD`、`--until YYYYMMDD`、`--json`、`--breakdown`、`--order asc|desc`、`--timezone UTC|local|+08:00`、`--locale en-US|zh-CN|ja-JP`、`--compact`、`--source codex|claude|opencode|gemini`。

运维命令：

- `llmusage init`
- `llmusage sync`（`--rebuild` 会重新解析本地真源并重建用量行/bucket；如果已导入的文件型历史存在缺失源文件，默认会拒绝执行；只有显式传 `--allow-lossy-rebuild` 才会清掉不可重建历史；默认进度写入 stderr，`--json-events` 会在 stdout 输出 NDJSON 生命周期/进度事件）
- `llmusage status`
- `llmusage diagnostics`（`--forget-file <PATH>` 可把源文件标记为用户主动忽略）
- `llmusage doctor`（`--refresh-pricing <file>` 导入本地价格快照并重算成本）
- `llmusage dash`
- `llmusage serve`
- `llmusage tui`（`dash` 的已废弃别名）
- `llmusage export html`
- `llmusage uninstall`

Web 分析页：

下面这张图就是 `llmusage serve` 启动后的本地浏览器分析页。首屏会把当前时间/来源/模型筛选、KPI、活动趋势、项目/模型/来源/成本排行、行为分析、同步/导出动作和诊断线索放在同一个本地只读视图里。

行为分析区由 `sync` 阶段提取的 normalized facts 驱动：

- Activity 和 Tools 聚合本地 turn/tool facts，不保存完整 prompt、assistant 消息或文件内容。
- Optimize 是只读建议面板，用于提示低 Read/Edit 比、重复读取、生成物/依赖目录读取、session outlier 等模式；不会自动删除、归档、重写或清理任何内容。
- Compare 会自动选择或接受两个模型，展示成本、cache、one-shot、retry、category 和工作风格指标，并明确提示低样本。
- 来源支持会显式降级：Claude/Codex 可产出更丰富的工具事实；Gemini/OpenCode 在源日志不提供工具级证据时只保守地产出 turn facts。

![llmusage 本地 web 分析页概览](./docs/public/screenshots/web-dashboard-overview.png)

终端分析页：

- `llmusage dash` 会打开终端 TUI。顶部导航现在包含 `8:行为`，用同一套 Activity / Tools / Optimize / Compare dashboard 查询展示只读行为摘要，并明确显示 no-data、degraded、insufficient-models、low-sample 状态，不把缺失事实伪装成 0。

开发：

```powershell
cargo check
cargo test
cargo run -- init
cargo run -- sync
cargo run -- --json
cargo run -- dash
cargo run -- serve
```

说明：

- `serve` 只监听 `127.0.0.1`，并会默认用系统浏览器打开分析页
- `serve` 支持单快照加载、URL 恢复筛选、可选 30s/60s 自动刷新，以及带进度/取消状态的进程内同步任务
- `export html` 生成同一套 Dashboard shell 的离线静态报告；离线快照会禁用实时 sync/refresh 控件
- 报表命令都是只读 SQLite 视图，不会自动 sync
- 行为分析在查询时仍是本地只读；它读取 sync 预提取的 `usage_turn` / `usage_tool_call`，不会在浏览器端解析 raw transcript
- 普通 `sync` 遇到源文件缺失时会保留已导入 usage history；diagnostics 里出现 `source_file.missing` 不代表 usage 行已被删除
- `status` 和普通 `diagnostics` 是只读命令；`diagnostics --forget-file` 会写入本地忽略状态
- 普通 `doctor` 是只读命令；`doctor --refresh-pricing <file>` 只读取本地 JSON，把快照保存到 `~/.llmusage/pricing/<catalog-version>.json`，并写入本地 SQLite 价格元信息/成本

## 0.5.1 重点

- 面向 ccr-ui 的只读 API：`Dashboard::overview`、`trends_daily`、`home_overview`、`heatmap`、`logs`、归档诊断与源文件 forget。
- 持久化成本列成为报表/查询真源：常规 sync 写入 event/bucket 成本元信息，`doctor --refresh-pricing <file>` 用本地快照同步重算 event 与 bucket，报表和 dashboard payload 暴露总成本、cache efficiency、每日成本、模型双价/pricing 元信息、项目成本以及日志 cost/id/recorded_at 字段。
- `JobRegistry` 提供进程内导入任务、进度快照与取消。
- v0/v1 到 v11 的完整 schema migration，覆盖 cache split、成本元信息、source_file 状态机、raw archive、worker lock 元信息、Gemini 注册、`pricing_catalog_version` 与 normalized behavior facts。
- CLI 报表、HTTP API、静态导出的 JSON 字段统一 snake_case。
- 为下游适配层提供公共 `LlmusageError` 和 `testing::Fixture`。

## 库 API（0.5.1）

0.5.x 为 ccr-ui 这类桌面适配层提供 SemVer-stable 的库表面：

```rust
use llmusage::{
    paths::AppPaths,
    query::{Dashboard, QueryFilter},
    store::Store,
    Result,
};

fn open_store(root: std::path::PathBuf) -> Result<Store> {
    let paths = AppPaths::with_root(root)?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok(store)
}

fn load_ccr_ui(store: &Store) -> Result<()> {
    let filter = QueryFilter::default();
    let dashboard = Dashboard::open(store)?;
    let _snapshot = dashboard.snapshot(&filter)?;
    let _overview = dashboard.overview(&filter)?;
    let _daily = dashboard.trends_daily(&filter)?;
    let _home = dashboard.home_overview(&filter)?;
    let _heatmap = dashboard.heatmap(&filter, 365)?;
    let _activity = dashboard.activity_breakdown(&filter)?;
    let _tools = dashboard.tool_breakdown(&filter)?;
    let _optimize = dashboard.optimize(&filter)?;
    let _compare = dashboard.model_compare(&filter, None, None)?;
    let _logs = dashboard.logs(&Default::default())?;
    Ok(())
}
```

路径解析顺序是 `--home <PATH>` 优先，其次 `LLMUSAGE_HOME`，最后 `~/.llmusage`。
ccr-ui 表面包含带 `QueryFilter` 的 dashboard/home/daily-trend/heatmap/log 查询、来自 `source_file` 状态机的归档诊断、持久化 cost/pricing/cache 字段、行为 activity/tool/optimize/compare payload，以及 `JobRegistry::start/get/cancel` 进程内导入任务。

下游适配层（如 ccr-ui）写集成测试时，可在 dev-dependencies 中启用测试夹具：

```toml
[dev-dependencies]
llmusage = { path = "../llmusage", features = ["testing"] }
```

```rust
let fixture = llmusage::testing::Fixture::new()?;
fixture.seed_dashboard(12)?;
let overview = llmusage::Dashboard::open(fixture.store())?.overview(&Default::default())?;
```
