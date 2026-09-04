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
| `~/.llmusage/backups/` | 历史集成备份以及数据库/价格恢复材料 |
| `~/.llmusage/exports/` | 静态 HTML 导出 |
| `~/.llmusage/logs/llmusage.ndjson.*` | 本地结构化运行诊断和命令追踪 |
| `~/.llmusage/pricing/` | 内容寻址的本地 base、overlay 和 effective 价格目录 |

运行时根目录优先级：`--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`。

当前版本不安装 hook 或 plugin。从会安装 hook 的旧版本升级后，请执行一次 `llmusage uninstall`，只移除 llmusage 自有的遗留配置条目、wrapper 和原子写入残留。历史 `*.bak` 与用量数据继续保留；只有显式执行 `uninstall --purge` 才会删除运行时根目录。

## 不上传什么

`llmusage` 不创建账号会话、device token、上传队列或远端用量 API 调用。报表、Dashboard 和导出都读取本地 SQLite。

可选的 SSH 远端导入由你触发：用 `llmusage remote add` 注册主机后，本机执行 `ssh` 并从 stdout 拉取规范化 shard 字段。不会传输 prompt 正文或原始会话 JSON。解析发生在拥有产物的那台机器上；只有本地数据库被写入。

项目 label 在本地推导。需要稳定分组的敏感路径维度会存为 hash。

运行诊断也只保存在本地。`LLMUSAGE_LOG` 控制 NDJSON 日志文件（`off`、`error`、`warn`、`info`、`debug`、`trace`，默认 `warn`），`RUST_LOG` 控制控制台 stderr。日志在单进程运行期间按 10 MiB 分片轮转，总量最多保留 30 MiB、7 个文件和 7 天；本地 `logs`/`diagnostics` 状态会报告保留文件与字节数、队列丢弃事件数以及轮转/保留失败数。运行日志会记录命令标签、run id、source、模块 target 和错误摘要；不会主动记录 prompt、response 或原始 source JSON。路径可能出现在人读错误摘要中，因此 diagnostics bundle 仍应当作本地排障材料处理。

可用 `llmusage logs --limit 50 --level warn` 跨保留分片查询最近运行日志和 SQLite `run_log` 记录。该命令只读取本地文件/数据库，不上传数据。轮转和保留清理会在进程持续写日志时执行，不需要重启触发。

## 普通 sync 保留数据

```powershell
llmusage sync
```

普通 sync 导入新增/变化的本地源记录。如果之前导入过的文件型来源现在缺失，sync 会保留已导入 usage history，并把源文件标为 missing 供 diagnostics 使用。

Web 与 TUI 的同步命令中心会把缺失文件/重建保护显示为中性事实，不会把它当成普通
sync 失败。标题只反映锁占用、最近一次 usage-import 失败、空状态或就绪；缺失文件数和
本地库保留事件数仍在同步详情中可追溯。前台同步会在释放 worker lock 前写入 `run_log`，
因此看板中的最近命令与 `finished_at` 会跟随本次运行一起更新。

## 普通 sync 自动修复安全的旧版 accounting

普通无界 `llmusage sync` 会检测本次所选 parser 来源是否仍使用旧版 token-accounting
合约。修改数据前先告警，并对全部自动目标检查缺失输入和受保护历史；全部安全时，只
reset legacy 子集，且每个所选来源只解析一次。

任一目标存在有损风险时，不会 reset 任何自动目标。请恢复源文件后重新运行普通 sync；
只有明确接受文档所述删除时才使用显式 rebuild 参数。`sync --recent-days N` 永远不会
自动修复旧版 accounting，因为全量 reset 后只做 bounded import 会丢掉窗口外历史。

## rebuild 可能有破坏性

```powershell
llmusage sync --rebuild
```

`--rebuild` 会按来源重置 parser-backed 用量状态，再重新解析本地来源。若重建会删除未归属的 hook 时代 Antigravity 行，即使带 `--allow-lossy-rebuild` 也会拒绝。如果 parser 来源的已导入文件型历史依赖现在缺失的源文件，llmusage 会在任何 reset 发生前拒绝重建。

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

普通 sync 与启动迁移两条自动路径都永远不会启用 `--allow-lossy-rebuild`，
parserless 来源也不是迁移目标。

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

`llmusage serve` 默认绑定 `127.0.0.1`。回环地址路由保留完整本地看板，包括项目、日志、诊断、同步游标状态、任务读取、行为分析、用量分析和带真实来源地址检查的写路由。

`llmusage serve --public` 会显式绑定 `0.0.0.0`，并选择独立的只读 router。只挂载页面 shell/静态资源、字段 allowlist 明确的聚合 `/api/dashboard` projection，以及固定的最小 `/api/health` 响应。原始日志、diagnostics、本地路径/project 字段、内部错误、job 状态和全部 mutation 路由都不存在，而不是依赖 `Host` 或 `Origin` header 保护。未来如需远程 diagnostics，必须另行提供显式 opt-in 和认证。

精简 public 视图仍不提供认证或 TLS，也会显示聚合用量、模型和来源数据。不要直接暴露给不受信任的网络；请使用防火墙或带认证的反向代理。远程需要完整 Dashboard 能力时，应优先通过 SSH 隧道访问默认 loopback 监听。

## 静态导出边界

`llmusage export html` 会写静态快照目录。`snapshot.json` 含 `projects`、`hosts`、模型、来源等聚合表，以及导出时捕获的 `recent_failures` 失败字符串。只有在你愿意分享这些项目标签、host、用量合计和失败文案时，才分享该目录。

实时 diagnostics 还含 `archive_root`（运行时根路径）。默认 HTML 导出会从 `snapshot.json` 剥离 `archive_root`。
