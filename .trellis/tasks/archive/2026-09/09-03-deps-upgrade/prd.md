# 依赖扫描与分批升级（2026-09-03）

## Goal

在不破坏 MSRV 1.95、现有 CLI/看板行为、以及 CI 门禁契约的前提下，摸清三个依赖生态相对 08-31 批次之后的过期项与安全风险，并按风险从低到高分批升级。每一批完成后必须通过完整本地门禁；失败则停在该批修复，不进入下一批。

用户价值：消化 08-31 之后出现的兼容补丁（Actions install-action、Cargo lockfile / tower-http），同时避免把 syn 3、工具链 1.98、VitePress 2 这类破坏性升级捆进来。

## Background

- 仓库有三套被 Dependabot 跟踪的依赖：根目录 Cargo、`docs/` npm（VitePress）、GitHub Actions SHA 钉扎。
- Python `scripts/` 与看板 `scripts/*.mjs` 没有第三方包清单；不纳入升级。
- `Cargo.toml` 声明 MSRV `rust-version = "1.95"`；日常工具链钉在 `1.97.0`。CI 用 `python scripts/ci-rust.py` 作为共享 Rust 门，并有独立 MSRV job。
- 归档任务 `08-31-deps-upgrade` 已完成：Actions rust-cache/rust-toolchain 钉扎、`cargo update`（含 `lru` 0.18.3）、docs `nanoid`/`postcss` overrides、reqwest 0.13。
- 扫描日（2026-09-03）证据见 `research/dependency-scan-2026-09-03.md`。`cargo audit` 无漏洞、无 yanked/unsound 警告。直接 crate 除 `tower-http` 0.7.0→0.7.1 与 dev-only `syn` 2→3 外均已是 crates.io 最新稳定版。

## Requirements

- R1. 升级计划必须覆盖 Cargo、docs npm、GitHub Actions，并标明过期、安全、废弃、可安全升级、以及带 Breaking Change 的项。
- R2. 实施必须分批，风险从低到高。每批只包含该批清单中的文件；不得把 Breaking Change 与 lockfile 兼容刷新捆在同一提交。本轮 Breaking Change（syn 3、VitePress 2、工具链 1.98）不实施。
- R3. 每一批结束后运行完整门禁 `just ci`。任一步失败则定位并修复后重跑该批门禁，通过后才允许开始下一批。
- R4. Cargo 批次必须保持 MSRV 1.95：`package.rust-version`、CI job 名 `MSRV (1.95)`、以及 `cargo +1.95 check --locked --all-features` 仍然成立。不得静默抬高 MSRV。
- R5. 依赖敏感的 Rust 命令继续使用 `--locked`。`Cargo.lock` 必须随 Cargo 批次一起提交。本轮不改 `docs/package-lock.json`，除非扫描后出现新的可修复 npm 告警（当前没有）。
- R6. `cargo audit` 不得新增未允许的 vulnerability。本轮基线已是 0 漏洞 / 0 警告；Cargo 批次后必须保持该基线。
- R7. docs npm 中已用 override 消除的 nanoid/postcss high 必须保持。vite/esbuild/vitepress 在 VitePress 1.6.4 无法无破坏修复的部分继续记为残留风险，不得为消告警而升级到 VitePress 2 alpha 或 override vite。
- R8. GitHub Actions 保持 SHA 钉扎风格；只刷新 `taiki-e/install-action` 到扫描记录的 v2.87.4。不改 `ci-gate` 的 job `name:`，不改 Node 20，不改 rust-cache / checkout / setup-node / rust-toolchain 钉（它们已是最新）。
- R9. 产品命令、看板 API、TUI、SQLite schema 的用户可见行为不得因依赖升级而改变。`tower-http` 0.7.1 只允许补丁吸收；本仓库只用 `CompressionLayer`，不得改静态资源路由语义。
- R10. 每批结束后若测试失败，修复范围限于该批引入的编译/测试/锁文件问题，不夹带无关重构。

## Acceptance Criteria

- [x] AC1. 任务 `research/` 中有一份扫描清单，列出三个生态的过期、安全、废弃、可安全升级、Breaking Change 分类（对应 R1）。
- [x] AC2. 实施按 `implement.md` 批次顺序进行；每一批有独立 diff，Breaking Change 不与兼容刷新同批（对应 R2）。
- [x] AC3. 每一批在继续下一批之前有 `just ci` 通过记录（对应 R3）。失败批已修复并重跑通过。
- [x] AC4. `Cargo.toml` 的 `rust-version` 仍为 `1.95`；CI MSRV job 仍安装 1.95 并 `cargo check --locked --all-features`（对应 R4）。
- [x] AC5. Cargo 锁文件已更新且随对应批次纳入工作区；Rust 门禁命令仍带 `--locked`（对应 R5）。实施代理未 git commit，除非用户在 Phase 3.4 确认。
- [x] AC6. Cargo 批次后 `cargo audit` 仍无 vulnerability 与未允许 warning（对应 R6）。
- [x] AC7. `docs/package.json` overrides 与 lock 中 nanoid 3.3.18 / postcss 8.5.26 保持不变；不新增 vite/esbuild override（对应 R7）。
- [x] AC8. `ci.yml` 中 install-action 钉到 v2.87.4 SHA `e67fa11c4b9316fa714ddf0abed07a0c3143b95b`；`CI gate` 名称未改（对应 R8）。
- [x] AC9. 全量 `just ci` 通过；`tower-http` 直接依赖为 0.7.1；订阅 HTTP 客户端仍编译（对应 R9）。
- [x] AC10. 无夹带的产品功能或无关重构（对应 R10）。

## Out of Scope

- 将 MSRV 从 1.95 升到 1.96+。
- 将开发工具链从 1.97.0 升到 1.98.0（不改 `rust-toolchain.toml` channel）。
- syn 2 → 3（架构测试 AST walker；高 churn、无用户价值）。
- VitePress 2.0 alpha、强制 vite 6/8 override。
- CI Node 20 → 22。
- 产品侧已有废弃项：`tui` 命令别名、`PricingCatalog::static_v1()`。
- Python / 看板 JS 引入 npm/pip 依赖。
- `ref/` 上游参考代码。
- 一次改完所有批次后只跑一次测试。

## Key Decisions

- 本任务包含：Actions install-action SHA 刷新、Cargo lockfile + `tower-http` 0.7.1 补丁对齐。
- 本任务不包含：工具链 1.98、syn 3、VitePress 2、MSRV 上调、docs npm 变更。
- `aws-lc-sys` 0.44 → 0.45 随 `cargo update` 吸收，不手工拆锁；若 Windows/Linux/macOS 任一 native 构建失败则整批回滚 `Cargo.toml` + `Cargo.lock`。
- 残留 vite/esbuild 告警视为文档开发服务器风险，等 VitePress 稳定线提供 vite ≥6.4.3 后再处理。
