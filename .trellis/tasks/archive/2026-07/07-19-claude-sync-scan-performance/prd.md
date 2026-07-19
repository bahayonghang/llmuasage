# 优化多来源同步扫描性能

## Goal

显著缩短 `llmusage sync` 在 Claude、Codex 与 OpenCode 三个注册 parser 来源上的扫描等待时间，并在不牺牲增量同步正确性、取消语义和 SQLite 单写者约束的前提下，消除各自热路径上经测量确认的主要性能放大。

## Background

- 用户在真实数据上观察到 Codex 处理 1794 个文件时很快完成，而 Claude 在显示“扫描 704 个文件...”后长时间等待。
- 当前 build 只有 Codex、Claude、OpenCode 三个注册 `SourceParser`（`src/registry.rs:23`）；Antigravity 等来源为 hook-only 或 monitor-only，不存在同类扫描循环。
- 此前已修复的数据库定价目录升级慢路径发生在来源扫描之前，本任务不重复处理该路径。
- 真实源与临时目标库的基线显示：
  - Claude 只出现 1 个变化文件时仍扫描 899,626,059 bytes、重放 79,270 个候选；总耗时约 106.5 秒，其中 parse 约 4.1 秒、write 约 102.1 秒。
  - Claude 的 704 个文件共约 895 MB；73,607 个 usage 候选中有 510 个 message ID 跨文件重复，但没有 message ID 跨顶层项目目录重复。
  - Codex 在约 2 GB 源中只处理 3 个变化文件、扫描约 7.4 MB，分来源同步约 1.0 秒；当前增量选择未发现全量放大。
  - OpenCode 源库约 90 MB，`part` 表 17,274 行；当前工具事实查询全表扫描并返回 5,728 行/约 35 MB，热查询约 0.6 秒，随后还会重复 JSON 解析和幂等写入。

## Requirements

- R1：分别建立可重复、可自动运行的 Claude、Codex 与 OpenCode 扫描反馈信号；优先断言扫描字节、处理行数、查询计划和工作集合上界，而非脆弱的绝对墙钟。
- R2：Claude 只重放包含实际变化文件的顶层项目，不得因一个文件追加而重放其他项目；同一项目内的流式/sidechain 跨文件去重结果必须保持不变。
- R3：Claude/Codex 的无变化文件必须保留游标增量、source-file 三态和 missing 历史数据；不得通过隐式全量 rebuild 获得性能改善。
- R4：OpenCode 数据库正常增长不得被误判为数据库替换；真实替换仍必须重置消息与工具事实水位。
- R5：OpenCode `part` 工具事实扫描必须采用持久化增量水位、有界分页和可取消处理；旧库缺少 `part` 表时继续优雅降级。
- R6：行为事实按 `source_path_hash` reset 必须走匹配索引；同一 shard 内重复 turn/tool key 在进入 SQLite 前去重，避免重复 `INSERT OR IGNORE`。
- R7：并发必须有明确上限，不得无界创建任务、文件句柄或数据库连接；SQLite 写入继续遵守全局 sync worker、单 `SyncRunWriter` 和 `SyncShard` 原子提交协议。
- R8：进度与统计必须反映真实工作量；若 Claude 项目级重放包含多个文件，这些文件不得被错误报告为 skipped；OpenCode 仅有新 tool part 时也必须报告来源发生了扫描工作。
- R9：只修复三个注册 parser 扫描主路径上经测量确认的问题，不做查询层、UI 或全仓库性能重构。

## Acceptance Criteria

- [x] A1/R1：修复前能稳定捕获 Claude 跨项目扫描放大、OpenCode 增长即换库和 `part` 全表扫描；Codex 基线证明只处理变化文件。
- [x] A2/R2,R3：多项目 Claude 夹具只扫描/重放发生变化的项目，其他项目保持 skipped；跨文件 streaming/sidechain 去重总量与首次全量导入一致。
- [x] A3/R3：Claude/Codex 首次导入、无变化、追加、截断/替换、missing/deleted、坏文件和取消路径全部通过。
- [x] A4/R4：OpenCode 原库追加后只处理新增消息，数据库替换后消息水位和 part 水位都安全归零并重新导入。
- [x] A5/R5：OpenCode 第二次无变化同步扫描 0 条 tool part；追加一个 tool part 后只处理新增范围，工具事实保持幂等完整。
- [x] A6/R6：迁移后行为表 reset 查询使用 `(source, source_path_hash)` 索引；重复行为事实预去重有 focused test。
- [x] A7/R8：三来源的 `files_processed`、`changed_files`、`skipped_files`、`bytes_scanned` 与实际工作一致，并保持旧 JSON 反序列化兼容。
- [x] A8/R1-R8：同一构建、同一夹具的修复后测量相对基线显著改善；若仍有主要耗时，必须记录证据而不能宣称完成。
- [x] A9：`cargo fmt --all -- --check`、严格 Clippy、相关串行测试、完整串行测试和 docs build 通过；无法运行的证据必须明确记录。

## Out Of Scope

- 查询层、Web/TUI 界面、hook-only/monitor-only 来源以及未注册候选来源的性能重构。
- 修改 OpenCode 源数据库的 schema 或索引；所有持久水位和索引只写入 llmusage 自有数据库。
- 新增运行时依赖、无界并发、并行 SQLite writer 或要求用户执行破坏性 `sync --rebuild`。
