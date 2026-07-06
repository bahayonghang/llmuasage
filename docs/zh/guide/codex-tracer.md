# Codex Tracer

当你需要只看 Codex 的逐调用 token 明细和线程关联，而不是主多来源 `llmusage serve` Dashboard 时，使用 `llmusage codex-tracer`。

## 它会做什么

`codex-tracer` 会读取本地 Codex rollout JSONL，写入一个独立的 SQLite 索引，然后启动本地浏览器 Dashboard。

默认路径：

| 项目 | 路径 |
| --- | --- |
| Codex rollout 根目录 | `$CODEX_HOME/rollout/` 或 `~/.codex/rollout/` |
| Tracer 数据库 | `~/.llmusage/codex-tracer.db` |
| 本地服务 | `127.0.0.1:8765` |

这个 tracer 数据库与 `llmusage.db` 分离。

## 基本用法

```powershell
llmusage codex-tracer
```

该命令会：

1. 扫描 `$CODEX_HOME/rollout/` 或 `~/.codex/rollout/` 下的 Codex JSONL 文件。
2. 构建或复用 `~/.llmusage/codex-tracer.db`。
3. 在 `127.0.0.1:8765` 启动本地服务。
4. 默认自动打开浏览器。

## 常用参数

```powershell
llmusage codex-tracer --port 9876
llmusage codex-tracer --no-open
llmusage codex-tracer --rebuild
```

| 参数 | 含义 |
| --- | --- |
| `--port <PORT>` | 改用其他端口启动本地 Dashboard 服务 |
| `--no-open` | 启动服务但不自动打开浏览器 |
| `--rebuild` | 删除 `codex-tracer.db`，然后从本地 JSONL 全量重建 |
| `--home <PATH>` | 覆盖 `~/.llmusage` 运行时根目录，`codex-tracer.db` 也跟着走 |

## 什么时候使用

- 主多来源 Dashboard 用 `llmusage serve`。
- 当你需要 Codex 专属细节，例如 cached/uncached input、reasoning output、线程内调用顺序时，用 `llmusage codex-tracer`。

## 前置条件与失败场景

- Codex 至少要在本机产出过一次 rollout JSONL。
- 如果 rollout 目录不存在，命令会报本地路径错误，并提示可以设置 `CODEX_HOME`。
- 如果没有发现任何 event，命令会直接退出，不启动 Dashboard。

## 相关入口

- [快速开始](./getting-started)
- [CLI 参考](../reference/cli)
- [架构说明](../architecture/)
