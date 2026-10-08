# Sync 结果表与来源恢复提示

## 决定

人读成功结果先打印一张连续来源表，`TOTAL` 之后再打印 warning 和 error。交互终端只在 Antigravity CLI 或 IDE 的修复会删除不可重建历史时询问；直接回车保留全部。接受丢失的产品在同一进程、同一把 worker 锁里重建，最终只打印反映该选择的一张表。

不弹选择的旧记账保持现在的整源跳过。Web、TUI、HTTP 和 `llmusage serve` 不能借这次交互放宽 `--allow-lossy-rebuild`。

2026-10-08 已确认两条边界，记录在 `research/recovery-decision.md`：

- 当场接受丢失只限 `antigravity` 和 `antigravity_ide`。其他旧记账来源没有选择。
- 带 `--recent-days` 的普通 sync 不询问，也不把该来源改成无窗口重建。

## 本轮覆盖事实

旧记账 parser 在 driver 之前就从写入集合移除。被移除的 CLI 不会再跑 family 发现，所以不能用 driver 的 `source_issues`，也不能用上次保存的诊断。已保存诊断可能不存在，也可能和现在的磁盘不一致。

worker 锁已经持有、legacy 来源已经移出写入集合、driver 和任何重建都还没开始时，对本次命令选中的 Antigravity CLI 和 IDE 做一次只读覆盖。入口放在 `src/parsers/antigravity.rs`，由现有 family 的阻断分类和这次提示共用，避免两套数量。

这次检查：

- 用现有 `list_antigravity_conversation_files` 和 `list_antigravity_ide_conversation_files` 列出 conversation `.db`。路径按 family 现在的规则规范化。CLI 清单在前，IDE 在后；同一路径只归第一次出现的根。
- 读取该来源已有的 tracked paths 和 file cursors，只用来对比，不写回。
- 新文件数是「落在该产品清单根下、且不在该产品 tracked 集合里」的路径数。不解码 protobuf，也不为了改归属去读 SQLite。跨根副本留在第一次出现的根，直到真正的解码阶段。
- 已跟踪但这次没有发现的路径只调用 `metadata`：能读到是范围外，`NotFound` 是缺失，其他 IO 错误是不可读。
- 清单根缺失以外的发现失败，或者根存在但不是目录，按现有 family 规则把两个产品都标成发现不完整。
- 保留条数用现有的按来源 `usage_event` 计数。
- 不调用 `snapshot`，不调用 `decode::read_file`，不写 usage、cursor、inventory 或记账版本，也不调用 `mark_inventory_seen`。

family 自己在全部阻断后返回前，仍然可以做现在的 fingerprint。本任务不删这条路径。只读提示路径不 fingerprint。legacy CLI 本来就不进入 family，所以这一轮不会因为它而做 fingerprint。

提示使用的数量只来自这次内存结果。下面两种夹具都要验收：

- 该来源没有旧的 `source_issues`，磁盘上仍有范围外 JSON 或缺失 `.db`。提示数量与磁盘一致，而且在用户选择之前没有 usage、cursor、inventory 或 marker 写入。
- 旧 `source_issues` 的缺失数或范围外数和磁盘不同。提示使用磁盘上的本轮数量。

不可读和发现不完整不能接受丢失。一个产品同时有缺失和不可读时，也不进入选择；接受后现有重建仍会拒绝，不能把这种拒绝包装成可选损失。

## 什么时候询问

询问条件同时满足：

- 当前命令是人读 `llmusage sync`，stdin、stdout、stderr 都是终端。
- 不是 `--json-events`，也不是已经带 `--rebuild` 的命令。
- 没有 `--recent-days`。有窗口时即使缺口可恢复也不询问。
- 上面的只读覆盖对 `antigravity` 或 `antigravity_ide` 给出了缺失或范围外，并且该产品没有不可读和发现不完整。

`--json-events`、stdin 管道、只重定向 stdout、只重定向 stderr，以及其他非终端，都不读 stdin。文件都在且能无损重建时也不问。Codex、Grok、Pi 和其他非 Antigravity 旧记账来源不问，即使它们也有缺失文件。

生产入口在 `src/commands/sync.rs` 的人读路径判断三个流。只有三个条件都成立才安装读取 stdin 的回调。engine 的默认回调是全部保留，library、serve、TUI 和 Web 不安装读 stdin 的回调。测试可以注入选择函数来覆盖保留、接受和失败回滚；注入函数不能代替生产入口测试。

生产入口还要证明这些情况没有调用 stdin 读取：stdout 单独重定向、stderr 单独重定向、stdin 是管道、`--json-events`。管道测试向 stdin 写入选择文本，结果仍必须是安全跳过。另有一个进程内读取器，在不允许询问时一旦被调用就返回错误，用来抓住「读了但忽略」的实现。

多个符合条件的产品按结果表的来源顺序逐个询问，问题写到 stderr，使用纯文本。`k` 保留，`r` 接受丢失，空输入保留该来源。非法输入只重问该来源一次，第二次仍非法则保留。Ctrl-C 取消整个选择阶段，不重建任何来源。

## 提示内容

每一问只包含来源、保留条数、缺口数量和会失去的历史类别：

- 范围外但仍存在的旧输入：写明数量，以及重建后只保留当前可解析文件。
- 磁盘上已经没有的跟踪库：写明缺失数量、尚未导入的新文件数量，以及重建会放弃只存在于缺失文件中的记录。

不打印路径、记录正文、path hash 或完整修复命令。新文件数只留在这次进程里，不加入 `SourceIssue`，也不加入 dashboard JSON。

## 同一次命令里的重建

普通同步先按现有规则跳过 legacy 来源，不提前删除。用户确认之后：

- 没人接受：driver 保持今天的行为。legacy 来源不在写入集合里；当前版本但被阻断的 IDE 仍走 family 的阻断返回。
- 有人接受：这些产品不进入前面的普通 driver。其余来源先完成普通同步。然后只把被接受的产品送进现有 `sync_antigravity_family`，参数是 `rebuild=true`、`allow_lossy_rebuild=true`、`recent_cutoff=None`。
- 不调用 `reset_sources_for_rebuild`。Antigravity 继续绕过预删除，由现有 `commit_antigravity_snapshot` 在自己的事务里提交。
- 未接受的产品不进入这次 snapshot。
- 范围外路径和缺失文件可以被这次接受覆盖。读失败、发现失败和取消仍然不能变成空快照。
- hook 时代无路径行继续保留。
- snapshot 失败时，该事务里的事件、cursor、inventory 和记账版本保持事务开始前的样子，不发出 `TokenAccountingRepairFinished`。已经提交的其他来源不因为这次失败被回滚。表后把该产品记为 error。
- 成功后用重建结果替换该来源在摘要中的统计，记账版本只按现有成功 snapshot 推进。拒绝的来源继续显示保留总量和 `COMMITTED 0`。

driver 对这次重建结果仍执行现有的收尾：补 lock wait、发出 `SourceFinished`；有 `last_error` 或已取消时不做 missing sweep。不另写一套库存规则。

带 `--recent-days` 时没有选择阶段，也没有上面的第二次 family 调用。原来的 cutoff 继续只作用于本次普通导入。

进度阶段不再先打一整段 legacy 警告。结构化日志可以保留一条短 warning。人读说明只出现在最终表后，同一来源不在进度和表后各打一遍长文。

## 表和表后说明

表的列、`TOTAL`、窄终端只压缩来源名、颜色只在 stdout 终端启用，这些保持不变。来源行之间不再插入 `↳`、parse issue 或警告。

被保护而没有提交的来源：

- `CHANGED` 和 `WRITE` 为 0。已读但随后丢弃的文件不算 changed。
- `COMMITTED` 为 0，`STORED` 是保留总量。
- `FILES` 是本轮只读覆盖归到该来源的发现文件数。阻断发生在解码前时，`BYTES` 为 0。family 的 fingerprint 不计入 `BYTES`。

表后每个来源至多一组说明，顺序跟表内来源顺序一致：

- 用户保留，或非交互下的可恢复 Antigravity 缺口：历史保留、缺失数和范围外数。非交互的一条动作是 `llmusage sync --rebuild --source <source> --allow-lossy-rebuild`。交互下已经选择保留时，动作是再次运行不带窗口的 `llmusage sync` 并选择接受。
- 带 `--recent-days` 的可恢复 Antigravity 缺口：不写重建已经发生。交互时的动作是去掉 `--recent-days` 后再运行 `llmusage sync`。非交互时的动作是上面的显式命令，并写明不要同时加 `--recent-days`。
- 其他旧记账来源：继续使用现有 `legacy_repair_warning`，动作是 `llmusage sync --rebuild --source <source>`。不承诺失败后可以原子恢复。
- warning 还包含 skipped 和 accounting。error 包含 malformed、oversized、读失败，以及用户已接受但重建失败。

说明不重复 tracing 原文。记录样本仍最多 8 条，放在对应来源的说明里；源级保护不生成 malformed 样本。非终端输出不含 ANSI。

legacy 来源本轮被跳过且命令没有取消时，把这次只读分类写进该来源已有的诊断 JSON，供重启后的 doctor 读到本轮缺口。写入时保留原来的记录级 parse issue 样本，不改 cursor、inventory、usage 或 marker。取消发生在状态保存之前时不写这份诊断。下一轮提示仍然重新做只读覆盖，不信任这份 JSON。

## 兼容

`SourceSyncStats`、`SyncEvent` 和 dashboard JSON 不增加字段。doctor、source-status、TUI、Web 继续读原有诊断，不获得交互提示。展示顺序变化不是诊断字段变化。

不改 token 字段、发现范围或无损旧记账的升级条件。隔离测试不得运行本机真实库。

## 契约修改边界

`token-accounting-contracts.md` 现在规定普通 sync 不重建、不推进 marker。实施时先改这份契约，再改 engine。新例外只有同时满足时才成立：人读终端的 stdin、stdout、stderr 都可用，不是 `--json-events`，命令本身没有 `--rebuild`，没有 `--recent-days`，用户对 `antigravity` 或 `antigravity_ide` 明确选择接受丢失。这个选择等价于在同一把锁里，只对这些产品执行无窗口的 `sync --rebuild --source <source> --allow-lossy-rebuild`。

没有这句确认、有时间窗口、非交互、非终端，以及其他来源，都维持现在的跳过。serve 仍然不重建。普通 sync 忽略命令行上的 `--allow-lossy-rebuild`；只有这次选择或本来就带 `--rebuild` 的显式命令才能使用有损边界。

随后再改 `source-sync-contracts.md` 的人读摘要：诊断移到 `TOTAL` 之后，询问条件包含 stdout 终端，有窗口的普通 sync 不在本命令里恢复 Antigravity 缺口。
