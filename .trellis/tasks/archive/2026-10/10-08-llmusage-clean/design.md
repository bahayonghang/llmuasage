# llmusage clean 设计

## Boundaries

`clean` 只读运行时根目录的目录项，并在 `--yes` 时删除方案 A 的文件。它不打开 SQLite，不拿 worker lock，不跑 migration，也不调用 `Store`。

命令注册在 `Commands`（`src/commands/mod.rs:54`），由 `dispatch` 转到新的 `commands::clean`。帮助文案写进 `ENGLISH_COMMANDS` 和 `CHINESE_COMMANDS`，放在 `doctor` / `logs` 附近。clap 的命令注释只负责 `llmusage clean --help`。

`src/lib.rs:99-107` 今天在分发前调用 `init_logging_for_paths`，而文件日志会创建 `logs/`。`clean` 必须改走只写 stderr 的日志初始化，避免空的 `--home` 被这条命令自己创建出来。轮转上限和保留规则不变。

`uninstall` 与 `src/store/schema.rs` 的复制逻辑不改。

## Classification

扫描使用 `symlink_metadata`，不跟随符号链接。分类函数接收名字、条目种类和是否位于 `backups/` 直接子级，便于测试不依赖操作系统是否允许创建符号链接。

种类只有普通文件、目录、符号链接、其他。符号链接一律是「未识别」。

已知类别聚成一行。不存在的类别省略。

| 类别 | 匹配 | 动作 |
| --- | --- | --- |
| 活库 | 根目录文件 `llmusage.db` | 保留 |
| 活库伴随文件 | 根目录文件 `llmusage.db-wal`、`llmusage.db-shm` | 保留 |
| 迁移副本 | `backups/` 的直接子文件，名字以 `llmusage.db.pre-` 开头，且不是下面的配置备份 | 可删除 |
| 配置备份 | `backups/` 的直接子文件，名字以 `.bak` 结尾，或正好是 `codex_notify_original.json` | 保留 |
| 研究基线 | 根目录目录 `baselines/` 下的普通文件 | 可删除 |
| 日志 | `logs/` | 保留 |
| 缓存 | `cache/` | 保留 |
| Codex tracer | 根目录文件 `codex-tracer.db` 及其 `-wal`、`-shm` | 保留 |
| 包装脚本 | `bin/` | 保留 |
| 导出 | `exports/` | 保留 |
| 锁 | 根目录文件 `worker.lock` | 保留 |
| 价表 | `pricing/`，以及根目录 `pricing-cache-litellm.json`、`pricing-cache-models-dev.json` | 保留 |
| 未识别 | 其余顶层条目；`backups/` 里不符合上面两条的直接子文件各自成行 | 未识别 |

配置备份规则优先于 `llmusage.db.pre-` 前缀。因此 `backups/llmusage.db.pre-custom.bak` 仍然保留。

`baselines/` 只删除普通文件，然后删除因此变空的目录。目录里的符号链接留下，并在表里显示为未识别。删除前把路径规范化；规范化结果必须仍在运行时根目录内，否则当作未识别。

`backups/` 目录本身不删除，也不递归进入子目录。

## Output

表格列：类别、路径、文件数、大小、最近修改、动作。大小同时打印精确字节和 `字节 / 1_000_000_000`、两位小数的 GB。不用 GiB，否则本机 1,596,936,192 字节约 1.49 GiB，读不成本次验收里的 1.60 GB。

最近修改时间取该行内普通文件的最大 mtime，格式 `YYYY-MM-DD HH:MM`，用本地时区。

表下加两行合计：全部已分类字节，以及其中动作为「可删除」的字节。

没有 `--yes` 时动作写计划。有 `--yes` 时先按同一分类删除，再打印实际删除的文件数和字节数。删除使用 `remove_file`，不使用会跟随目录符号链接的递归删除。单个失败记下来，其余继续；有失败则退出码非 0。

根目录不存在，或没有任何目录项时，只打印「没有可清理内容」，退出码 0。

文案用中文，和 `doctor` 的操作输出一致。命令名仍是 `clean`。

## Data flow

1. `AppPaths::with_cli_home(cli.home)` 得到根目录。
2. 根目录缺失则打印空结果并返回。
3. 读取直接子项并分类。`baselines/` 只向下统计普通文件。
4. 渲染表格。
5. `--yes` 时按「可删除」名单删除，再打印结果。

不读文件内容。不计算哈希。

## Compatibility

- 全局 `--home` 的含义不变。
- `LLMUSAGE_HOME` 继续只由 `AppPaths::discover` 读取。`clean` 不调用 `discover`。
- schema v0 / v22 的一次性复制仍会在升级时重新生成对应文件。已升级库上的旧副本删掉后不会自动回来。这是方案 A 的已知代价。
- hook / `uninstall` 仍不得批量删除 `backups/*.bak`，也不得顺手删除 `llmusage.db.pre-0.5.0`。

集成契约在同一改动中补一句：`integration-file-contracts.md:53-55` 只约束 hook 清理和 `uninstall`。`llmusage clean` 无 `--yes` 时不删除任何文件；`llmusage clean --yes` 可以删除 `backups/llmusage.db.pre-*` 及其 wal/shm，以及产品布局之外的 `baselines/`。它仍然不能删除 `backups/*.bak` 和 `backups/codex_notify_original.json`。

## Trade-offs

- 默认不删除，避免把 `uninstall --purge` 那种一次性破坏带到日常总览上。`--yes` 与 `remote remove --delete-usage` 相同，不再做第二次交互确认。
- 不复用 `src/tui/report_table.rs`。那套列模型是用量报表，不是磁盘类别。
- 不提供 JSON。需求只要人读总览。
- 不把 `exports/`、日志和缓存标成可删除。本机这些目录几乎不占空间，删它们也不解决 6 GB 的旧库副本。

## Rollback

没有数据库迁移。回退就是去掉 `clean` 命令、帮助行、stderr-only 日志入口，以及契约里新加的那一句。已经用 `--yes` 删掉的副本不能由这次回退恢复。
