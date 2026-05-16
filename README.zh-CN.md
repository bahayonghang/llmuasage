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

`llmusage` / `llmusage daily` 默认按所选时区显示过去 7 个自然日（包含今天）；human 输出会按 Source 分彩色表（优先 Codex、Claude，有 OpenCode/Gemini 数据时继续追加），表之间用 `---` 分隔，token 使用 K/M/B 短格式；`NO_COLOR=1` 会禁用 ANSI 样式，JSON 输出保持 snake_case 和聚合 daily payload 不变。`Notes` 列会标注未定价模型以及 Claude reasoning token 未上报等来源侧缺口。需要完整 daily 历史时使用 `--all`，需要指定范围时使用 `--since YYYYMMDD` / `--until YYYYMMDD`。

常用报表参数包括 `--since YYYYMMDD`、`--until YYYYMMDD`、`--json`、`--breakdown`、`--order asc|desc`、`--timezone UTC|local|+08:00`、`--locale en-US|zh-CN|ja-JP`、`--compact`、`--source codex|claude|opencode|gemini`。

运维命令：

- `llmusage init`
- `llmusage sync`（`--rebuild` 会重新解析本地真源并重建用量行/bucket；如果已导入的文件型历史存在缺失源文件，默认会拒绝执行；只有显式传 `--allow-lossy-rebuild` 才会清掉不可重建历史；默认进度写入 stderr，`--json-events` 会在 stdout 输出 NDJSON 生命周期/进度事件）
- `llmusage status`
- `llmusage diagnostics`（`--forget-file <PATH>` 可把源文件标记为用户主动忽略）
- `llmusage doctor`（`--refresh-pricing <file>` 导入本地价格快照并重算成本）
- `llmusage serve`
- `llmusage tui`
- `llmusage export html`
- `llmusage uninstall`

Web 分析页：

下面这张图就是 `llmusage serve` 启动后的本地浏览器分析页。

![llmusage 本地 web 分析页概览](./docs/public/screenshots/web-dashboard-overview.png)

开发：

```powershell
cargo check
cargo test
cargo run -- init
cargo run -- sync
cargo run -- --json
cargo run -- serve
```

说明：

- `serve` 只监听 `127.0.0.1`，并会默认用系统浏览器打开分析页
- `export html` 生成离线静态报告
- 报表命令都是只读 SQLite 视图，不会自动 sync
- 普通 `sync` 遇到源文件缺失时会保留已导入 usage history；diagnostics 里出现 `source_file.missing` 不代表 usage 行已被删除
- `status` 和普通 `diagnostics` 是只读命令；`diagnostics --forget-file` 会写入本地忽略状态
- 普通 `doctor` 是只读命令；`doctor --refresh-pricing <file>` 只读取本地 JSON，把快照保存到 `~/.llmusage/pricing/<catalog-version>.json`，并写入本地 SQLite 价格元信息/成本

## 0.5.1 重点

- 面向 ccr-ui 的只读 API：`Dashboard::overview`、`trends_daily`、`home_overview`、`heatmap`、`logs`、归档诊断与源文件 forget。
- 持久化成本列成为报表/查询真源：常规 sync 写入 event/bucket 成本元信息，`doctor --refresh-pricing <file>` 用本地快照同步重算 event 与 bucket，报表和 dashboard payload 暴露总成本、cache efficiency、每日成本、模型双价/pricing 元信息、项目成本以及日志 cost/id/recorded_at 字段。
- `JobRegistry` 提供进程内导入任务、进度快照与取消。
- v0/v1 到 v10 的完整 schema migration，覆盖 cache split、成本元信息、source_file 状态机、raw archive、worker lock 元信息、Gemini 注册与 `pricing_catalog_version`。
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
    let _overview = dashboard.overview(&filter)?;
    let _daily = dashboard.trends_daily(&filter)?;
    let _home = dashboard.home_overview(&filter)?;
    let _heatmap = dashboard.heatmap(&filter, 365)?;
    let _logs = dashboard.logs(&Default::default())?;
    Ok(())
}
```

路径解析顺序是 `--home <PATH>` 优先，其次 `LLMUSAGE_HOME`，最后 `~/.llmusage`。
ccr-ui 表面包含带 `QueryFilter` 的 dashboard/home/daily-trend/heatmap/log 查询、来自 `source_file` 状态机的归档诊断、持久化 cost/pricing/cache 字段，以及 `JobRegistry::start/get/cancel` 进程内导入任务。

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
