# 添加 llmusage clean 命令

## Goal

用户运行 `llmusage clean` 就能看到 `~/.llmusage` 里各类文件占了多少空间。只有 `llmusage clean --yes` 会删除已经确认的过期数据库副本。按方案 A，本机大约可以收回 6.01 GB；活库和集成配置备份留下。

## Background

- 运行时根目录默认是 `~/.llmusage`，可用全局 `--home` 覆盖。本机 `LLMUSAGE_HOME` 未设置，实际目录是 `C:\Users\lyh\.llmusage`。名字、字节数和修改时间见 `research/local-home-inventory.md`。
- 当前没有 `clean`。`doctor` 只读；`uninstall --purge` 删除整个根目录；`remote remove --delete-usage` 必须带 `--yes`（`src/commands/remote.rs:118`）。
- 顶层帮助是手写双语表，不是 clap 生成的帮助。英文命令在 `ENGLISH_COMMANDS`（`src/commands/help.rs:294`），中文命令在 `CHINESE_COMMANDS`（`src/commands/help.rs:463`）。
- CLI 用 `AppPaths::with_cli_home`。`None` 时固定走 `~/.llmusage`，不读 `LLMUSAGE_HOME`（`src/runtime/paths.rs:55-59`，调用点 `src/lib.rs:105`）。
- 集成清理契约禁止按 `backups/*.bak` 做批量删除，并要求 `llmusage.db.pre-0.5.0` 对 hook / `uninstall` 仍然留着（`.trellis/spec/llmusage/backend/integration-file-contracts.md:53-55`）。这约束的是那条清理路径，不是用户已确认的 `clean --yes`。
- 当前代码只在 schema v0 复制 `backups/llmusage.db.pre-0.5.0`，在 schema v22 复制 `backups/llmusage.db.pre-0.23-host`（`src/store/schema.rs:163-168`、`412-418`）。`pre-accounting-*` 和 `pre-schema-*` 不是当前代码生成的。`baselines/` 不在 `AppPaths` 里。

## Requirements

- R1. 新增 `llmusage clean`。没有 `--yes` 时只打印总览，不创建、不修改、不删除运行时文件。进程入口也不得为了这条命令创建 `logs/`。
- R2. 总览是一张表，列至少包括类别、路径、文件数、字节数、最近修改时间、计划动作（保留 / 可删除 / 未识别）。字节数同时用 `字节 / 10^9`、保留两位小数的 GB 显示。覆盖活库、活库 wal/shm、`backups/` 里的迁移副本、`backups/` 里的配置备份、`baselines/`、日志、缓存、`codex-tracer.db`、`bin/`、`exports/`、`worker.lock`、价表，以及不属于已知布局的其他条目。不存在的类别省略，不打印全 0 行。
- R3. 运行时根目录不存在，或存在但没有任何文件和子目录时，命令成功退出，并说明没有可清理内容。
- R4. 遵守全局 `--home`。未传 `--home` 时不得读取 `LLMUSAGE_HOME`。
- R5. 只有 `--yes` 才删除。未带该标志时，标成「可删除」的文件也必须原样留下。
- R6. 任何模式下都不删除：`llmusage.db`、`llmusage.db-wal`、`llmusage.db-shm`、`backups/*.bak`、`backups/codex_notify_original.json`、`codex-tracer.db`、`logs/`、`cache/`、`bin/`、`exports/`、`worker.lock`、`pricing/`，以及根目录上的价表获取缓存。
- R7. 未识别路径只展示，不删除。符号链接即使名字符合删除规则，也标成未识别，并且不删除、不跟随。
- R8. 英文和中文顶层帮助都列出 `clean`。
- R9. `--yes` 的删除集合是方案 A：`backups/` 的直接子文件里，文件名以 `llmusage.db.pre-` 开头、且不属于 R6 的文件，包括 wal/shm 伴随文件；以及 `baselines/` 目录中的普通文件。删除后打印文件数和释放的字节数。某个文件删除失败时继续处理其余文件，最后以非零状态退出并写出失败路径。

## Acceptance Criteria

- [ ] AC1. 在临时目录夹具上不带 `--yes` 运行时，夹具文件的数量和字节数不变。输出能读出活库、迁移副本和 `baselines/`。1,596,936,192 字节显示为 1.60 GB，4,846,220,884 字节量级显示为 4.85 GB，1,160,123,446 字节显示为 1.16 GB。
- [ ] AC2. 对一个尚不存在的临时 `--home`，命令成功退出，说明没有可清理内容，并且没有创建该目录。
- [ ] AC3. 带 `--yes` 时只删除 R9 的集合。R6 列出的路径和未识别文件都还在。
- [ ] AC4. `llmusage --help` 和 `llmusage help --zh` 的命令表都出现 `clean`。
- [ ] AC5. 名字是 `baselines` 或 `llmusage.db.pre-*` 的符号链接，在分类结果里是未识别；`--yes` 不删除它。

## Out of Scope

- `uninstall --purge`，以及 hook / plugin 清理路径。
- 压缩或重建活库 `llmusage.db`。
- 清理第三方工具自己的目录，例如 `~/.claude`、`~/.codex`。
- 改日志轮转上限，或把 0 字节日志分片当成垃圾删除。
- 为 `pricing/` 增加新的回收策略。价表文件只展示并保留。
- JSON 输出，以及删除前的交互确认。

## Technical Notes

- 用户已确认方案 A：连当前代码仍会生成的 `pre-0.5.0` 和 `pre-0.23-host` 一起删除。`*.bak` 与 `codex_notify_original.json` 继续保留。
- 删除这两份回滚副本后，当前代码只在库仍处于 schema v0 或 v22 时才会重新复制。已经升过级的库不会自动恢复这两份文件。
- 实现同一改动里要给集成清理契约加一句范围说明：`integration-file-contracts.md:53-55` 的保留规则继续约束 hook / `uninstall`，不约束 `llmusage clean --yes`。否则检查会按旧契约把方案 A 改回去。
