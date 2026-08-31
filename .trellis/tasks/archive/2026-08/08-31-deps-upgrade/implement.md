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

本地结果见 `research/batch-results.md`。四批均绿，无回滚。

## Batch 0 — GitHub Actions SHA（最低风险）

- [x] rust-cache → `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` # v2.9.2
- [x] install-action → `1ed6d7be6168f6c9046541087ff549b6bc581fdf` # v2.87.2
- [x] dtolnay/rust-toolchain → `6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772`
- [x] 不改 toolchain 字符串 `1.97.0` / `1.95`，不改 Node 20，不改 job 名
- [x] 验证：`python scripts/check-ci-gate.py --self-test`、`python scripts/check-ci-gate.py`、`just ci`（全部 exit 0）

文件：`.github/workflows/ci.yml` 仅 `uses:` 行。

回滚：还原 `ci.yml`。

## Batch 1 — Cargo lockfile + 补丁号对齐（低风险，含安全修复）

- [x] `cargo update`（尊重 rust-version 1.95）
- [x] 将 `Cargo.toml` 中已解析到的补丁号对齐：clap `4.6.6`、rusqlite `0.40.2`、thiserror `2.0.20`、base64 `0.23.1`
- [x] 确认 `Cargo.lock` 中 `lru` ≥ 0.18.2（0.18.3）、`chacha20` ≥ 0.10.2（0.10.2）
- [x] 不改 reqwest、syn 主版本（本批 reqwest 仍 0.12.28）
- [x] 验证：`cargo audit`、`cargo +1.95 check --locked --all-features`、`just ci`（全部 exit 0）

风险点：`miniz_oxide` 0.8 → 0.9 与新增 `zlib-rs` 是 0.x 传递升级。测试通过，未回滚。

回滚：还原 `Cargo.toml` + `Cargo.lock`。

## Batch 2 — docs npm 可修复告警（低–中，仅文档工具链）

- [x] 增加 overrides：`nanoid` `3.3.18`、`postcss` `8.5.26`
- [x] `npm --prefix docs install` 刷新 lock
- [x] 不碰 vitepress 版本、不加 vite override
- [x] 验证：`npm --prefix docs audit`（nanoid/postcss high 消失；vite/esbuild 残留）、`npm --prefix docs run docs:build`、`just ci`（docs:build 与 just ci exit 0）

回滚：还原这两个文件。

## Batch 3 — reqwest 0.13（最高本轮风险 / Breaking Change）

- [x] `Cargo.toml`：`0.12` → `0.13`，feature `rustls-tls` → `rustls`，保留 `json`、`http2`、`default-features = false`
- [x] `cargo update -p reqwest`（锁定 0.13.4）
- [x] 调用点无需修改：`src/subscription/**` 现有 `Client::builder` / `json()` / `StatusCode` 即可编译
- [x] 未同时改 syn 或工具链
- [x] 验证：`python scripts/ci-rust.py`、`cargo +1.95 check --locked --all-features`、`just ci`（全部 exit 0）。Windows aws-lc 构建成功。

回滚：还原 `Cargo.toml` + `Cargo.lock` 中 reqwest 及其 TLS 闭包；保留 Batch 1 的其它 lock 更新。未触发。

## 明确不做

- `rust-toolchain.toml` 1.97.0 → 1.98.0
- syn 2 → 3
- VitePress 2 alpha
- CI Node 20 → 22
- 抬 MSRV

## 完成前检查

- [x] `implement.md` 四批均打勾或 Batch 3 因构建阻塞已回滚并记入 research（四批均绿）
- [x] AC 清单可逐项对照
- [x] 无产品行为 diff（除 reqwest feature 名）
