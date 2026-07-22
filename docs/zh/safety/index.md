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
| `~/.llmusage/pricing/` | 内容寻址的本地 base、overlay 和 effective 价格目录 |

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

`--rebuild` 会按来源重置 parser-backed 用量状态，再重新解析本地来源。无 source 的 full rebuild 以 parser registry 作为删除边界，因此 parserless Antigravity 的 event、bucket、行为事实、cursor 和 source-file 诊断都会保留。如果 parser 来源的已导入文件型历史依赖现在缺失的源文件，llmusage 会在任何 reset 发生前拒绝重建。

显式覆盖参数是：

```powershell
llmusage sync --rebuild --allow-lossy-rebuild
```

只有当你接受清掉不可重建历史时才使用。

## Dashboard 启动迁移

`llmusage serve` 会在绑定本地端口前检查 parser-backed 来源是否使用旧版 token 统计
口径。只有追踪的输入文件仍然可用时，才会自动逐源重建。存在有损重建风险的来源会告警并
跳过：历史仍可读取，普通写入继续被 guard 拒绝，Dashboard 也会继续启动。来源通过安全
预检后若发生意外错误，则会终止启动。

启动自动迁移永远不会启用 `--allow-lossy-rebuild`，parserless 来源也不是迁移目标。

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

## 价格目录变更只读本地文件

```powershell
llmusage catalog apply .\pricing-overlay.json
llmusage catalog status --json
llmusage catalog reset
llmusage doctor --refresh-pricing .\litellm-prices.json
```

`catalog apply` 激活增量 v2 overlay；`doctor --refresh-pricing` 激活完整 base snapshot 并清除已有 overlay。两者只接受已存在的本地文件，拒绝 URL，也不会联网拉取。

激活会在 `~/.llmusage/pricing/` 下写入 SHA-256 内容寻址文件，重算本地 event 和 bucket 成本，随后切换 SQLite catalog metadata。已选择文件缺失、被修改或无效时会显式报错，不会静默回退内置价格。`catalog reset` 移除 overlay，并用它记录的 base 重算。未被引用的 digest 文件可以作为本地审计材料保留；`uninstall --purge` 会随整个运行时根目录一起删除。

## 浏览器 Dashboard 边界

`llmusage serve` 默认绑定 `127.0.0.1`，只在进程运行期间暴露本地 HTTP endpoints。`llmusage serve --public` 会显式绑定 `0.0.0.0`，暴露不带认证和 TLS 的 Dashboard 与 JSON API。不要直接暴露给不受信任的网络；请使用防火墙、SSH 隧道或带认证的反向代理限制访问。

## 静态导出边界

`llmusage export html` 会写静态快照目录。只有在你愿意分享 `snapshot.json` 中聚合用量值和 label 时，才分享该目录。
