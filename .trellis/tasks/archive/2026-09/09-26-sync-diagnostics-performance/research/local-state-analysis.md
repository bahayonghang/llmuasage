# 本轮运行与 Antigravity 数据保护分析

## 证据范围

- 用户原始日志时间：2026-09-26T08:31:47.086575Z。日志为本轮分析的起始证据。
- `llmusage --version` 输出 `llmusage 1.3.0`，命令解析到用户 Cargo bin 目录。工作区 HEAD 为 `29bde59e84e1149b1d9215e020352586fb0a0ed7`，Cargo 版本为 1.4.0。安装二进制的构建提交尚未确定。
- `git diff 07221f7..HEAD -- src tests` 为空；这说明当前源文件与 Antigravity 功能提交一致，不能据此证明安装二进制对应某个提交。
- `local-state-probe.py` 只读打开用户数据库，使用 `mode=ro&immutable=1`；拒绝有 WAL 的数据库，检查读前后 size/mtime 与 WAL 状态。结果写入同目录 `local-state-probe.json`。未调用真实 sync、rebuild、reset 或安装命令。
- 探针只查询 accounting 标记、路径存在性、状态计数与运行计时，不导出消息正文、用量明细或用户配置。
- 本轮 source_sync_status 的更新时间均为 2026-09-26T08:33:53Z，数值与用户日志一致。当前文件存在性于 08:59:22 UTC 核查；这些计数不证明文件恰在同步当时丢失，也不确定删除原因。

## 运行结果与计时

| 项目 | 已核实数值 | 含义与限制 |
| --- | ---: | --- |
| run_log duration_ms | 126999 | 约 127 秒；run_log 的秒级时间戳计算不构成毫秒精度 profile |
| PARSE 合计 | 20375 ms | 包含扫描、cursor/fingerprint、解析等待及计时外工作，不能当纯解析 CPU |
| WRITE 合计 | 105193 ms | writer 计时范围内累计时间，需分段测量原因 |
| Codex + Claude WRITE | 103199 ms | 占 WRITE 的约 98.1% |
| WRITE / (PARSE + WRITE) | 约 83.8% | 优先研究 writer 的依据，不证明磁盘、锁或某 SQL 是根因 |
| BYTES 合计 | 2710299261 | 各来源统计语义不同，不能直接计算物理磁盘吞吐率 |
| SQLite worker lock wait | 用户日志 58 ms | 锁获取成功；不是本轮约 127 秒的主要时间来源 |
| run_log status | success | 编排完成；不代表每个来源都写入了新数据 |

终端命令之前的提示符 `⏱ 6s` 不应作为本轮 sync 耗时。计数、去重和写入边界详见 `performance-analysis.md`。

## A1：CLI legacy accounting 是现有保护行为

本地 `token_accounting_version.antigravity=2`，当前 parser 契约为 3；IDE 标记为 3。CLI 本轮 `files_processed=0`、`events_inserted=0`，`stored_events=3477`。

`src/sync/engine.rs:228-244` 在 ordinary sync 检测旧标记，发出警告并从可写集合排除旧来源。ADR 0017 的决定要求显式 rebuild，禁止把 v3 新统计直接混入旧口径。重复警告不能靠自动升级 marker 或静默重算消除。

后续维护入口仍为 `llmusage sync --rebuild --source antigravity`，但本轮没有执行。执行前必须核实可重建范围；`--allow-lossy-rebuild` 只能用于明确接受丢失的操作，不能作为默认优化。

当前 CLI 有 216 条跟踪路径，包含 104 个 `.db` 与 112 个 `.json`；路径存在性检查为 0 个缺失，当前 native root 有 111 个 `.db`。旧 `source_file` 状态仍有 112 条 missing。物理存在、当前 reader 可发现和可恢复旧事件是不同条件，不能由“0 个不存在路径”推导“可无损 rebuild”。`source_files.rs:272-300` 只发现 native `.db`；具体旧路径的语义迁移需通过隔离用例验证。

## A2：IDE 输入不完整，保留历史但暂停整组导入

当前已跟踪与 cursor 成员各为 501，存在性检查发现 147 个 `.db` 缺失，当前 native root 有 500 个 `.db`。501 条持久化 source_file 状态仍为 live。文件被删除、轮转或移动的外部原因未查明。数量 500 不足以证明上游实行 500 文件保留策略。

`src/parsers/antigravity.rs:175-198` 合并 tracked_paths 与 cursor 成员；`:240-253` 检测任何未发现的旧成员，标记整组失败。`:440-444` 跳过失败来源的提交，因而本轮仍保留 9591 条历史，新增写入为 0。完整产品组保证 copied DB、请求去重和跨产品归属的一致性；不能改成删除缺失文件对应历史或部分覆盖。

`src/sync/engine.rs:651` 遇到 last_error 跳过 missing sweep，因此既有 live 状态不证明当前文件存在。诊断应同时说明“上次成功观察的库存状态”和“本次预检观察”，不通过推进 usage cursor 或删历史来刷新诊断。

## A3：来源级错误被误计为 malformed

`src/parsers/antigravity.rs:145-149` 将任何 record_failure 同时写为 last_error 与 `ParseIssueKind::Malformed`。缺成员属于来源完整性问题，没有证据表明出现一行语法损坏。当前 `malformed=1` 因此是分类缺陷。

自由文本 reason 经过安全 reason-code 清洗后变为 `Antigravitytrackeddatabasemissinghistorypreservedrestoreitorusee`，不能保留可操作的错误含义。修复需要稳定错误码、来源级计数、缺失成员总数和有限定位样本。原始正文与完整私有路径不应进入默认日志或 dashboard JSON。

last_error 目前主要存在于当轮 SourceSyncStats；持久化诊断使用已有 parse_issues_json。不能只移除 malformed 记录而丢失重启后的错误证据。具体兼容契约由诊断子任务定义并测试旧 JSON 的读取。

## A4：已阻断的产品仍执行完整解码

缺成员判断位于 `antigravity.rs:240-253`，但之后仍进入 `:294-370` 解码循环。成功解码后 `:350-359` 累加 files_processed、changed_files 和 file_size，最终 `:440-444` 才跳过提交。本轮 IDE 500 个成功解码成员均被计为 changed，PARSE=6866 ms、WRITE=0。

`bytes_scanned` 在 `:353` 累加主数据库 file_size，394.0 MiB 是数据库逻辑大小之和，不能证明实际磁盘读取 394.0 MiB。优化应优先减少确知无法提交的组的 decode 次数，保留必要的轻量发现、fingerprint 和产品归属核验。

不能只按文件夹归属提前过滤。`antigravity.rs:153-154` 及 ADR 0017 要求同时检查两个 root，native metadata 优先于目录，跨 root 副本可能属于仍可同步的来源。首个安全快速路径可限定“所有选中的产品均已阻断”；部分阻断的跳过只在已有证据足以证明归属与身份影响范围时启用，未知成员继续走安全路径。

## A5：锁提示与 WARN 混行

用户日志首行确认发生混行。`src/commands/sync_progress.rs:179-194` 的行渲染器已有 LockAcquired 换行；`:271-276,412-420` 的 TTY bar 使用永久行。`src/runtime/logging.rs:214-215` 的 tracing 与 `src/sync/engine.rs:237-242` 的 warn/eprintln 是独立输出路径。

当前源码足以识别多个 stderr 写入者，但混行的具体竞态及安装版本行为尚未复现。不能声称缺少一个换行就是根因。任务验收应覆盖真实 renderer 与 warning 连续/交错输出、TTY 与 non-TTY、NDJSON stdout 纯净性。

## 保留契约与验证边界

- 完整产品组 replay、selected-source 限制、跨产品原子提交、DB/WAL fingerprint、bounded cursor、不丢 hook-era 行，全部来自当前 ADR/spec，不从参考项目替换。
- 隔离回归入口包括 `antigravity_missing_group_member_preserves_bounded_and_forgotten_history`、`antigravity_native_product_survives_cross_root_copy_and_unreadable_file`、`antigravity_wal_only_commit_replays_snapshot`、`antigravity_rebuild_accepts_raw_windows_cursor_path_without_lossy_flag`。见 `tests/sync/sources/antigravity.rs:274,390,451,553`。
- 本轮没有运行 Cargo 测试、重放真实用量库、测量实际 SQLite 各阶段，也没有实施修复。动态证据是现有运行记录和只读库存核查；后续测试命令在各子任务 implement.md。
- 先前决策检索来源：Basic Memory permalink `basic-memory/decisions/llmusage/antigravity-cli-and-ide-native-accounting-v3`；已使用当前 `docs/adr/0017-antigravity-native-accounting.md` 与代码核实关键约束。
