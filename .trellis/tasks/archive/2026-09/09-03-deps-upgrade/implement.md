# 实施清单

在 `task.py start` 之后按批次执行。**不要合并批次。** 每批完成后跑 `just ci`；失败则只修该批，重跑通过后再继续。

完整门禁：

```text
just ci
```

等价于：`python scripts/check-ci-gate.py --self-test`、`python scripts/check-ci-gate.py`、`python scripts/ci-rust.py`、dashboard `node --check` / `node --test`、`npm --prefix docs run docs:build`。

Cargo 批次额外（隔离 target，证明 MSRV）：

```text
cargo +1.95 check --locked --all-features
```

以及 `cargo audit`。

结果写入 `research/batch-results.md`。

## Batch 0 — GitHub Actions SHA（最低风险）

- [x] install-action → `e67fa11c4b9316fa714ddf0abed07a0c3143b95b` # v2.87.4（两处 `uses:`：arch-gate 与 security）
- [x] 不改 toolchain 字符串 `1.97.0` / `1.95`，不改 Node 20，不改 job 名，不改其它 action SHA
- [x] 验证：`python scripts/check-ci-gate.py --self-test`、`python scripts/check-ci-gate.py`、`just ci`

文件：`.github/workflows/ci.yml` 仅 `uses:` 行与版本注释。

回滚：还原 `ci.yml`。

## Batch 1 — Cargo lockfile + tower-http 补丁对齐（低风险；含 aws-lc-sys 0.x）

- [x] `cargo update`（尊重 rust-version 1.95）
- [x] 将 `Cargo.toml` 中 `tower-http` 写成 `0.7.1`
- [x] 确认 lock：`tower-http` 0.7.1、`lru` 0.18.4、`aws-lc-sys` 0.45.0、`smallvec` 1.16.0；`reqwest` 仍 0.13.4；直接 `syn` 仍 2.x
- [x] 不改 syn 主版本、不改 rust-toolchain、不改 reqwest feature
- [x] 验证：`cargo audit`、`cargo +1.95 check --locked --all-features`、`just ci`

风险点：`aws-lc-sys` 0.44 → 0.45 是 0.x 传递升级，Windows native 构建失败则整批回滚。

回滚：还原 `Cargo.toml` + `Cargo.lock`。

## 本轮不做的批次

- docs npm（无可兼容修复的新告警）
- reqwest（已在 08-31 升到 0.13.4）
- syn 3 / 工具链 1.98 / VitePress 2 / Node 22 / 抬 MSRV

## 完成前检查

- [x] `implement.md` 两批均打勾，或 Cargo 批因构建阻塞已回滚并记入 research
- [x] AC 清单可逐项对照
- [x] 无产品行为 diff
