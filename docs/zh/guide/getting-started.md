# 快速开始

`llmusage` 是一个本地优先的 Rust CLI。

## 环境要求

- Rust stable
- Node.js 20+
- npm 10+
- `just`

## 安装依赖

```powershell
just install
```

这条命令会：

- 安装 `docs/` 下的 VitePress 依赖
- 用 `cargo install --path . --locked --force` 安装当前仓库里的 CLI

## 跑通本地链路

```powershell
llmusage init
llmusage sync
llmusage
llmusage serve
```

### 每一步做什么

- `init` 建立 `~/.llmusage/`、创建 `llmusage.db`、生成 hook 包装器并安装 Codex / Claude / OpenCode / Gemini 集成。
- `sync` 增量解析本地真源并写入 SQLite。人读进度默认写入 stderr；脚本需要生命周期/进度事件时可用 `--json-events` 输出 NDJSON。
- 不带子命令的 `llmusage` 会从本地 DB 输出 daily 报表。也可以使用 `llmusage daily --json`、`llmusage monthly`、`llmusage session`、`llmusage blocks` 查看其他报表。
- `serve` 在 `127.0.0.1` 上启动本地分析页，并默认用系统浏览器打开它。

报表命令都是只读操作，不上传数据，也不会自动 sync；源数据变化后请重新运行 `llmusage sync`。可用 `--source codex|claude|opencode|gemini` 限定报表或同步来源。升级后如果需要重新填充 session/source-file metadata，可运行 `llmusage sync --rebuild`。

## 回归检查

```powershell
just ci
```

`ci` 会运行格式检查、clippy、测试和 VitePress 生产构建。

## 库 API 预览

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
    let _home = dashboard.home_overview(&filter)?;
    let _heatmap = dashboard.heatmap(&filter, 365)?;
    let _logs = dashboard.logs(&Default::default())?;
    Ok(())
}
```

运行根目录解析顺序是 `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`。
0.5.0 的 ccr-ui 表面包含 `Dashboard::overview`、`home_overview`、`heatmap`、`logs`、来自 `source_file` 状态机的归档诊断，以及通过 `JobRegistry` 暴露的进程内导入任务。CLI 与 library 入口共用运行根目录解析顺序：`--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`。

下游适配层可在集成测试中启用测试夹具：

```toml
[dev-dependencies]
llmusage = { path = "../llmusage", features = ["testing"] }
```

```rust
let fixture = llmusage::testing::Fixture::new()?;
fixture.seed_dashboard(12)?;
let overview = llmusage::Dashboard::open(fixture.store())?.overview(&Default::default())?;
```
