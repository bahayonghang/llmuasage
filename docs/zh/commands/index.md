# 命令参考

## 核心命令

### `llmusage init`

初始化本地运行时、创建 SQLite、生成 hook 包装器，并安装 Codex / Claude / OpenCode 三类集成。

### `llmusage sync`

顺序执行三类本地解析器，把增量结果写入 30 分钟 bucket。

### `llmusage status`

输出人读摘要：数据库路径、bucket 数、最近同步、来源用量、集成状态、最近错误。

### `llmusage diagnostics`

输出机器可读 JSON，包括路径、集成状态、SQLite、cursor、来源统计、健康检查和最近运行记录。

### `llmusage doctor`

只读健康检查，覆盖包装器缺失、集成漂移、OpenCode DB 缺失、最近失败等问题。

## 本地界面命令

### `llmusage serve`

在 `127.0.0.1` 启动本地分析页和 JSON API。

### `llmusage tui`

打开终端摘要与运维面板。

### `llmusage export html`

导出静态目录：

- `index.html`
- `snapshot.json`
- `assets/app.css`
- `assets/app.js`

## 卸载

### `llmusage uninstall`

恢复被改过的集成配置。只有加 `--purge` 时才会删除 `~/.llmusage/`。
