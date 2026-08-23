# 设计：跨子任务的迁移、集成与回滚

本文件只处理四个子任务共用的迁移面。记录级解析规则、成本折算、行为提取的设计各自
写在子任务的 `design.md`。

## D1 三条迁移路径与各自的门

R2「不双计」必须同时在三条路径上成立。事实核对如下。

| 路径 | 现有行为 | 结论 |
| --- | --- | --- |
| 默认 `sync` | 先按 `--source` 过滤出 `parsers`（`src/commands/sync.rs:617`），再用过滤后的 `parser_sources` 检查 legacy（`:635`）。未过滤时包含 `Pi`，自动重建生效 | 提 Pi 版本即可 |
| `sync --source omp` | `parser_sources` 只有 `Omp`，完全不检查 `Pi` | 需要新增门，否则新 `omp:` 行与旧 `pi:` 行并存 |
| 远端主机导入 | `reset_for_source(source, host_id)` 是 host 作用域（`src/store/schema.rs:256`），`reset_sources_for_rebuild` 固定传 `"local"`（`src/commands/sync.rs:920`），并有测试断言「rebuild resets local only」（`tests/remote_lifecycle.rs:285`）。`token_accounting_version.<source>` 存在 `meta` 且不分 host（`src/store/schema.rs:360`），本地一次重建成功后 `mark_current_token_accounting` 就把标记转正（`src/commands/sync.rs:813`），远端 `pi` 行从此不再被检出 | 需要 host 级迁移机制 |

### D1.1 默认 sync

`expected_token_accounting_version` 给 `Pi` 单独分支、版本 2 → 3，`Omp` 用
`TOKEN_ACCOUNTING_VERSION`。链条：`has_legacy_token_accounting(Pi)` 为真 →
`automatic_token_accounting_repair_sources` 纳入 `Pi` → `reset_for_source(Pi,"local")`
清事件、桶、行为事实与游标 → `Pi` 只重放 `.pi` 根 → `Omp` 作为新源全量解析 `.omp`。

`Omp` 自身不会触发误报：`has_legacy_token_accounting` 在 `usage_event` 行数为 0 时
返回 false（`src/store/schema.rs:196`），新源第一次同步没有行。

### D1.2 限定源同步：拒绝而不是隐式扩大作用域

在 `parsers` 过滤之后、流水线之前加一道门：

```text
if parser_sources 含 Omp 且不含 Pi 且 store.has_legacy_token_accounting(Pi) {
    bail!("拒绝 `--source omp`：存量 pi 行仍是拆分前的记账口径，先执行一次
           `llmusage sync`（不带 --source、不带 --recent-days）完成 pi→omp 迁移。")
}
```

选择拒绝的理由：让 `--source omp` 隐式重建 `pi` 会把 `--source` 的作用域悄悄放大，
而且有界 sync 本来就拒绝自动重建（`src/commands/sync.rs:938` 已有同风格先例）。
拒绝是显式的、可测的，且不会破坏任何既有语义。

同一道门也覆盖 `--source pi`：这条路径本身会被既有 legacy 检查处理，不需额外分支。

### D1.3 远端主机：host 级一次性迁移

机制：在 `meta` 增加 host 级标记 `omp_split_migrated.<host_id>`。远端导入路径
（`src/remote/importer.rs`）在提交来自 host H 的 shard 之前判定：

```text
if shard.source == Omp 且 meta 无 omp_split_migrated.H {
    reset_for_source(Pi, H)          // 清掉该 host 拆分前导入的 pi 行
    set_meta_value(omp_split_migrated.H, run_started_at)
}
```

选择理由：远端主机的升级节奏不受本地控制，一旦对端升级到拆分版本，它就会开始上报
`omp` shard，这个信号本身就是「对端已切换」的证据，用它做一次性 host 迁移最精确。

被否方案：

- 要求用户对每个 host 执行 `llmusage remote remove <label> --delete-usage` 再重新注册。
  否决理由：会丢掉该 host 全部源的历史，代价远超本次拆分。
- 在导入路径检测混合版本并整体拒绝。否决理由：对端仍在旧版时会让导入完全停摆。

兜底文档：若 host 级迁移在实现中被证明不可行，则必须在 `docs/` 与命令输出里写明手动
步骤，并让 AC6 指向该文档路径 —— 二选一，不允许两边都不做。

## D2 子任务 2/3/4 的历史回填口径

普通增量 `sync` 不会回填：游标已在子任务 1 的同步中写入，`should_rescan_file` 会跳过
未变化文件（`src/parsers/pi.rs:134`、`:146`）。因此：

- 不给子任务 2/3/4 各自提 token accounting 版本号。理由：该版本号表达的是 **token
  记账契约**，而 provider、project、成本、行为都不改 token 口径；ADR 0010 已经确立
  「新增维度的历史归属需要显式 `sync --rebuild`」的先例。
- 子任务 2/3/4 的本机验证与验收统一使用 `llmusage sync --rebuild --source omp`。
  该路径走 `reset_for_rebuild` 与 `lossy_rebuild_risks` 守护：`.omp` 文件都在磁盘上时
  不阻塞；文件已被删除时按既有语义要求 `--allow-lossy-rebuild`。
- 发布说明必须写这一条，否则用户升级后只有新会话带新维度。

## D3 集成顺序

四个子任务在同一发布窗口内合并，发布说明只写一次迁移指引：

1. 子任务 1 合并（含 D1.1/D1.2/D1.3 机制）。
2. 子任务 2、3 合并（顺序无关，串行避免同文件冲突）。
3. 子任务 4 合并。
4. 本任务执行最终集成检查（见 `implement.md`）。

如果只发布了子任务 1 就要出版本，是允许的：此时 `omp` 源可用、成本与维度仍为空，
与当前状态相比没有回退。

## D4 跨子任务回滚

回滚顺序与前进顺序相反。关键点：代码回退不会自动清理已写入的 `omp` 行。

1. 回退代码到拆分之前。
2. 对每个 host 清 `omp` 行：本地 `reset_for_source(Omp,"local")`，远端逐 host 执行；
   同时删除 `meta` 里的 `omp_split_migrated.*` 与 `token_accounting_version.omp`。
3. 删除 `token_accounting_version.pi`（回到未标记状态），再跑一次完整 `llmusage sync`，
   让旧代码按合并口径重建 `pi`。
4. 若第 2 步前已删除 `.omp` 会话文件，则那部分历史无法重建，需从备份恢复数据库。

因此 `implement.md` 要求在执行任何升级 sync 之前先备份数据库并导出身份基线。
