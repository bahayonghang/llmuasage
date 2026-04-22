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
llmusage serve
```

### 每一步做什么

- `init` 建立 `~/.llmusage/`、创建 `llmusage.db`、生成 hook 包装器并安装三类集成。
- `sync` 增量解析本地真源并写入 SQLite。
- `serve` 在 `127.0.0.1` 上启动本地分析页。

## 回归检查

```powershell
just ci
```

`ci` 会运行格式检查、clippy、测试和 VitePress 生产构建。
