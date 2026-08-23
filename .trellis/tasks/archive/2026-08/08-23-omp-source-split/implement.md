# 执行计划：拆出 omp 独立源

## 前置

- 读 `.trellis/spec/llmusage/backend/source-sync-contracts.md`、
  `token-accounting-contracts.md`、`write-fencing-contracts.md`。
- 读 `docs/agents/domain.md` 与 `docs/agents/passive-parser-onboarding.md`。
- 读父任务 `design.md`（三条迁移路径）与 `implement.md`（基线导出步骤）。
- 参照归档任务 `.trellis/tasks/archive/2026-08/08-16-deepseek-harness-descriptor/`
  的新增源改动面。
- **先完成父任务 `implement.md` 的「升级前必须留下的基线」四步**（备份 + 身份基线
  + 聚合基线 + 真源扫描存档），再执行任何升级 sync。

## 步骤

1. [x] 加 `SourceKind::Omp`（`src/domain/models.rs`）：枚举值、`as_str`、clap value name。
       跑 `cargo check` 收集所有非穷尽 match 报错，作为编译期接线点清单。
2. [x] 补 `src/domain/source_descriptor.rs` 的 `omp` 描述符；确认
       `parse_source_id("omp")` 返回 `Some(Omp)` 并补单测（AC1.1）。
3. [x] 拆分 `src/parsers/source_files.rs`：`list_pi_session_files()` 只留 pi 根，
       新增 `list_omp_session_files()`，按 canonical 路径做**双向**前缀判定跳过冲突文件，
       跳过数记入 `errors`。单测覆盖三种重叠形态（AC1.2、AC1.3）。
4. [x] 参数化 `src/parsers/pi.rs`：结构体持有 `source` 与 `list_files`，
       替换全部 `SourceKind::Pi` 字面量与 `event_key` 前缀，日志与 `SyncEvent`
       用 `self.source`。单测：两源 `event_key` 不同、各自幂等（AC1.5）。
5. [x] `src/registry.rs` 注册两个实例；`src/parsers/mod.rs` 的
       `bounded_contract_parse` 映射补 `Omp`（AC1.9）。
6. [x] `src/store/schema.rs`：`Pi` 单独分支提到 3，`Omp` 用 `TOKEN_ACCOUNTING_VERSION`。
7. [x] 限定源门（`src/commands/sync.rs`）：`parsers` 过滤之后插入 R1.7 的拒绝分支，
       错误信息指明先跑不带 `--source` 的完整 sync。集成测试覆盖拒绝与迁移后放行（AC1.6）。
8. [x] 远端 host 迁移（`src/remote/importer.rs`）：`meta` 键
       `omp_split_migrated.<host_id>`，首次导入该 host 的 `Omp` shard 时在同一写事务里
       `reset_for_source(Pi, host_id)` 并写标记。补 `src/commands/remote.rs:134` 的
       源枚举数组。测试：清理生效、标记幂等（AC1.7）。
       若实现受阻，改走文档兜底并把 AC1.7 指向文档与命令输出。
9. [x] 补 `src/domain/platform_monitor.rs`：`pi` 描述符去掉 `.omp` 根，新增 `omp` 描述符。
10. [x] 硬编码源清单（编译器发现不了，逐个改）：`src/commands/help.rs:413`、
        `src/commands/diagnostics.rs:132`、`src/tui/report_table.rs` 配色、
        `src/web/assets/components.css` 源色块（用 Bash 改，不用 Edit）。
        改完用全仓搜索确认没有遗漏：
        `rg -n "codex.*claude.*opencode" --glob '!target' --glob '!docs/.vitepress/dist'`。
11. [x] `tests/sync_regression.rs`：新增用例覆盖「存量 pi 事件在 pi 提版本后被重置，
        `.omp` 文件在 omp 源下重放，身份集合被覆盖」（AC1.4）。
12. [x] 拒绝信息：确认自动重建被 lossy 守护拒绝时的文案能指明 pi→omp 拆分；
        需要则在 `src/commands/sync.rs` 增补一条针对 `Pi` 的说明分支。
13. [x] 文档（AC1.8）：`README.md`、`README.zh-CN.md`、`docs/index.md`、
        `docs/zh/index.md`、`docs/guide/first-sync.md`、`docs/architecture/index.md`、
        `docs/reference/cli.md`、`docs/dashboard/index.md`、
        `docs/agents/passive-source-candidates.md` 与 `docs/zh/` 对应页。
        首同步文档要写明 R8 的 `sync --rebuild --source omp` 回填口径。
14. [x] 新增 `docs/adr/0015-omp-source-split.md`（AC1.10）并在 `docs/adr/index.md` 登记：
        拆分决定、三条迁移路径、回滚步骤。

## 验证命令

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
cargo run -- sync            # 不带 --source、不带 --recent-days
cargo run -- source-status
cargo run -- daily --source omp
just ci
```

本机数据校验（只读查询，配合父任务导出的身份基线）：

```sql
SELECT source, COUNT(*), SUM(total_tokens) FROM usage_event
WHERE source IN ('pi','omp') GROUP BY 1;
-- 期望：pi 为 0 行；omp 覆盖基线身份集合（差集查询见父任务 implement.md 步骤 2）
```

## 评审门

- 步骤 3 完成后先跑三种重叠形态的单测，再继续。
- 步骤 4 完成后先跑步骤 11 的回归用例，再做门与远端迁移。
- 步骤 7、8 是双计防线，必须各自有测试通过后才执行本机升级 sync。
- 本机升级 sync 后先核对身份集合差集，再继续文档与 ADR。

## 回滚点

- 步骤 1–12 属于纯代码改动，`git restore` 即可。
- 本机库已被重建后：按父任务 `design.md` D4 的四步回滚，
  或直接从步骤 0 的备份恢复 `~/.llmusage/llmusage.db`。
