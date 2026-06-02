# 安全说明

`llmusage` 围绕本地优先边界设计。本页列出数据路径，以及可能写入或删除本地状态的命令。

## 本地数据路径

默认运行时根目录：

```text
~/.llmusage/
```

常见文件和目录：

| 路径 | 用途 |
| --- | --- |
| `~/.llmusage/llmusage.db` | 保存 usage、bucket、cursor、diagnostics、jobs、run logs 和 metadata 的 SQLite 数据库 |
| `~/.llmusage/bin/llmusage-hook.cmd` | Windows hook wrapper |
| `~/.llmusage/bin/llmusage-hook.sh` | POSIX hook wrapper |
| `~/.llmusage/backups/` | 卸载时使用的集成配置备份 |
| `~/.llmusage/exports/` | 静态 HTML 导出 |
| `~/.llmusage/logs/llmusage.ndjson` | 本地结构化运行诊断和命令追踪 |
| `~/.llmusage/pricing/` | `doctor --refresh-pricing` 导入的本地价格快照 |

运行时根目录优先级：`--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`。

## 不上传什么

`llmusage` 不创建账号会话、device token、上传队列或远端用量 API 调用。报表、Dashboard 和导出都读取本地 SQLite。

项目 label 在本地推导。需要稳定分组的敏感路径维度会存为 hash。

运行诊断也只保存在本地。`LLMUSAGE_LOG` 控制 NDJSON 日志文件（`off`、`error`、`warn`、`info`、`debug`、`trace`，默认 `warn`），`RUST_LOG` 控制控制台 stderr。运行日志会记录命令标签、run id、source、模块 target 和错误摘要；不会主动记录 prompt、response 或原始 source JSON。路径可能出现在人读错误摘要中，因此 diagnostics bundle 仍应当作本地排障材料处理。

可用 `llmusage logs --limit 50 --level warn` 查询最近运行日志和 SQLite `run_log` 记录。该命令只读取本地文件/数据库，不上传数据。活动日志文件采用保守上限：启动时如果超过 10 MiB，会轮转为 `llmusage.ndjson.old`；不再需要的旧日志可手动删除。

## 普通 sync 保留数据

```powershell
llmusage sync
```

普通 sync 导入新增/变化的本地源记录。如果之前导入过的文件型来源现在缺失，sync 会保留已导入 usage history，并把源文件标为 missing 供 diagnostics 使用。

## rebuild 可能有破坏性

```powershell
llmusage sync --rebuild
```

`--rebuild` 会在重新解析前删除可重建 usage 行、bucket、project 行和 cursor。如果已导入文件型历史依赖现在缺失的源文件，llmusage 默认拒绝重建。

显式覆盖参数是：

```powershell
llmusage sync --rebuild --allow-lossy-rebuild
```

只有当你接受清掉不可重建历史时才使用。

## 诊断缺失源文件

```powershell
llmusage diagnostics --out .\llmusage-diagnostics.json
```

diagnostics 包含 source-file archive 状态，例如 missing file count、protected event count 和 lossy rebuild risk。

如果某个源文件应被主动忽略，使用显式写入入口：

```powershell
llmusage diagnostics --forget-file <PATH> --source codex
```

这会把该行标记为 `deleted_by_user`，并移除 cursor 行。

## 价格刷新只读本地文件

```powershell
llmusage doctor --refresh-pricing .\litellm-prices.json
```

该命令会把本地 JSON 快照复制到 `~/.llmusage/pricing/`，重算本地成本列，并记录 `pricing_catalog_version`。URL 会被拒绝。

## 浏览器 Dashboard 边界

`llmusage serve` 只绑定 `127.0.0.1`。进程运行期间会暴露本地 HTTP endpoints；不会打开公网监听。

## 静态导出边界

`llmusage export html` 会写静态快照目录。只有在你愿意分享 `snapshot.json` 中聚合用量值和 label 时，才分享该目录。
