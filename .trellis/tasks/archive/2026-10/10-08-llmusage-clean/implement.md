# llmusage clean 执行计划

## Checklist

1. 读 `prd.md`、`design.md` 和 `research/` 三份记录。不要改 `src/store/schema.rs` 或 `uninstall`。
2. 在 `src/lib.rs` 的 `run` 里，`clean` 跳过 `init_logging_for_paths`。缺文件日志入口时，在 `src/runtime/logging.rs` 加一个只挂 stderr、不创建 `logs/` 的初始化。不要改轮转上限。
3. 给 `Commands` 增加 `Clean { yes: bool }`，默认 `yes` 为 false。`dispatch` 调用 `commands::clean`，不要创建 `Store`。
4. 实现扫描、纯分类函数、表格和 `--yes` 删除。分类规则以 `design.md` 的表为准。配置备份优先于 `llmusage.db.pre-` 前缀。
5. 英文和中文顶层帮助都加上 `clean`。
6. 在 `.trellis/spec/llmusage/backend/integration-file-contracts.md` 的保留规则后加 `design.md` 里的范围说明。不要放宽 hook / `uninstall` 对 `*.bak` 和 `pre-0.5.0` 的现有测试。
7. 补测试，再跑下面的验证命令。

## Tests

夹具用临时目录，不要指向 `C:\Users\lyh\.llmusage`。

- 纯分类：活库、wal/shm、`pre-0.5.0`、`pre-0.23-host`、`pre-accounting` sqlite 及其 wal/shm、`*.bak`、`codex_notify_original.json`、`baselines/` 里的普通文件、`codex-tracer.db`、缓存、日志、未知文件。
- `backups/llmusage.db.pre-custom.bak` 归配置备份，不是可删除。
- 符号链接种类即使名字匹配，也是未识别。用分类函数的条目种类覆盖，不要求测试进程有创建符号链接的权限。
- 不带 `--yes` 时夹具字节数和文件数不变。
- 带 `--yes` 后只少了迁移副本和 `baselines/` 普通文件。R6 的路径还在。
- 根目录不存在时输出「没有可清理内容」，且目录仍不存在。
- 1,596,936,192、4,846,220,884、1,160,123,446 分别显示为 1.60 GB、4.85 GB、1.16 GB。
- `top_level_help_with_width` 的英文和中文输出都包含 `clean`。
- clap 能解析 `clean` 与 `clean --yes`。

## Validation

```bash
cargo test --locked --lib clean -- --test-threads=1
cargo test --locked --lib commands::help::tests -- --test-threads=1
python scripts/ci-rust.py
```

库测试过滤名以实际模块路径为准。共享门禁是 `python scripts/ci-rust.py`。

## Risky files

- `src/lib.rs`：日志初始化顺序。改错会让所有命令停写文件日志，或让 `clean` 创建空的 `--home`。
- `src/commands/mod.rs`：命令枚举和分发。
- `src/commands/help.rs`：两份命令表必须一起改。
- `src/runtime/logging.rs`：只允许新增不创建目录的入口。
- `.trellis/spec/llmusage/backend/integration-file-contracts.md`：只加范围说明。

不改 `src/store/schema.rs`、`src/commands/uninstall.rs`。

## Rollback

上述文件回退即可。没有迁移。`--yes` 已经删掉的用户文件不在代码回退范围内。

## Before start

- `implement.jsonl` 和 `check.jsonl` 需要有真实条目。
- 用户确认本规划后才能 `task.py start`。
