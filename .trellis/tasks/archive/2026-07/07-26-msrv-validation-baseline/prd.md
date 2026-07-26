# MSRV 与验证基线诚实化

## Goal

恢复可重复、可解释的本地/CI 验证基线，使 `rust-version`、MSRV job、格式 gate 和 subprocess 集测反映真实支持状态。

## Confirmed Evidence

- `Cargo.toml:5` 声明 Rust 1.85，CI 的 MSRV job 使用 1.89。
- 实测 `cargo +1.85.0 check --locked --all-features` 因依赖要求 1.88 失败；1.88 又在 `libsqlite3-sys` 失败。
- 当前 `cargo fmt --check` 在 `src/commands/sync.rs` 和 `src/sync/job_registry.rs` 失败。
- `tests/m2_raw_archive_logs.rs:809` 的 `os error 2` 来自读取已失效的固定日志路径，
  不是 binary spawn；runtime 实际写入按日滚动的 `llmusage.ndjson.YYYY-MM-DD`。
- Rust 1.89 到 1.94 均因 `libsqlite3-sys 0.38.1` 使用尚未稳定的
  `cfg_select!` 失败；Rust 1.95 是第一个通过 locked all-features check 的版本。

## Requirements

- 以最小实际可构建版本作为唯一 MSRV，`Cargo.toml`、CI、toolchain/docs 同步。
- MSRV job 必须使用 `--locked --all-features`，不得只检查默认 feature。
- 修复格式基线，但不夹带无关 reformat。
- 复现并归因 subprocess path failure；修复 test harness/environment seam，而不是删除、忽略或降低断言。
- 本地 `just ci` 与 GitHub CI 使用相同核心命令来源。

## Acceptance Criteria

- [x] 隔离 target 上 `cargo +1.95.0 check --locked --all-features` 成功。
- [x] 1.94 的 `cfg_select!` unstable feature 失败已记录，MSRV 选择可解释。
- [x] `cargo fmt --check` 成功且 diff 仅包含必要格式变化。
- [x] `json_events_subprocess_emits_ndjson_per_event` 在 Windows 独立运行及全套运行均稳定通过。
- [x] 完整 Rust tests 不再含未归因的 `os error 2`。

## Out of Scope

- 不为保留 1.85 大规模降级依赖或牺牲已使用功能；优先声明真实 MSRV。
