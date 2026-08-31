# 依赖扫描与分批升级

## Goal

在不破坏 MSRV 1.95、现有 CLI/看板行为、以及 CI 门禁契约的前提下，摸清三个依赖生态的过期项与安全风险，并按风险从低到高分批升级。每一批完成后必须通过完整本地门禁；失败则停在该批修复，不进入下一批。

用户价值：减少已知安全告警和陈旧传递依赖，同时避免一次大升级把回归范围搅乱。

## Background

- 仓库有三套被 Dependabot 跟踪的依赖：根目录 Cargo、`docs/` npm（VitePress）、GitHub Actions SHA 钉扎。
- Python `scripts/` 与看板 `scripts/*.mjs` 没有第三方包清单；不纳入升级。
- `Cargo.toml` 声明 MSRV `rust-version = "1.95"`；日常工具链钉在 `1.97.0`。CI 用 `python scripts/ci-rust.py` 作为共享 Rust 门，并有独立 MSRV job。
- 扫描日（2026-08-31）证据见 `research/dependency-scan-2026-08-31.md`。

## Requirements

- R1. 升级计划必须覆盖 Cargo、docs npm、GitHub Actions，并标明过期、安全、废弃 API、可安全升级、以及带 Breaking Change 的项。
- R2. 实施必须分批，风险从低到高。每批只包含该批清单中的文件；不得把 Breaking Change 与 lockfile 兼容刷新捆在同一提交。
- R3. 每一批结束后运行完整门禁 `just ci`。任一步失败则定位并修复后重跑该批门禁，通过后才允许开始下一批。
- R4. Cargo 批次必须保持 MSRV 1.95：`package.rust-version`、CI job 名 `MSRV (1.95)`、以及 `cargo +1.95 check --locked --all-features` 仍然成立。不得静默抬高 MSRV。
- R5. 依赖敏感的 Rust 命令继续使用 `--locked`。`Cargo.lock` / `docs/package-lock.json` 必须随对应批次一起提交。
- R6. `cargo audit` 不得新增未允许的 vulnerability。本任务至少消化 lockfile 可解的 `lru` unsound 警告（RUSTSEC-2026-0253）。
- R7. docs npm 中 **已有修复** 的 high 告警（nanoid、postcss）必须用兼容 override 或等价 lock 更新消除。vite/esbuild/vitepress 在 VitePress 1.6.4 无法无破坏修复的部分记录为残留风险，不得为消告警而升级到 VitePress 2 alpha。
- R8. GitHub Actions 保持 SHA 钉扎风格；刷新 rust-cache、install-action、dtolnay/rust-toolchain 到扫描记录的最新兼容提交。不改 `ci-gate` 的 job `name:`。
- R9. 产品命令、看板 API、TUI、SQLite schema 的用户可见行为不得因依赖升级而改变。reqwest 批次只允许 TLS 后端/feature 名按 0.13 changelog 迁移，订阅拉取语义不变。
- R10. 每批结束后若测试失败，修复范围限于该批引入的编译/测试/锁文件问题，不夹带无关重构。

## Acceptance Criteria

- [x] AC1. 任务 `research/` 中有一份扫描清单，列出三个生态的过期、安全、废弃、可安全升级、Breaking Change 分类（对应 R1）。
- [x] AC2. 实施按 `implement.md` 批次顺序进行；每一批有独立 diff，Breaking Change 不与兼容刷新同批（对应 R2）。
- [x] AC3. 每一批在继续下一批之前有 `just ci` 通过记录（对应 R3）。失败批已修复并重跑通过。
- [x] AC4. `Cargo.toml` 的 `rust-version` 仍为 `1.95`；CI MSRV job 仍安装 1.95 并 `cargo check --locked --all-features`（对应 R4）。
- [x] AC5. 相关锁文件已更新且随对应批次纳入工作区；Rust 门禁命令仍带 `--locked`（对应 R5）。实施代理未 git commit。
- [x] AC6. `cargo audit` 无未允许 vulnerability；`lru` 已升到 ≥0.18.2（对应 R6）。
- [x] AC7. `npm --prefix docs audit` 不再报告 nanoid/postcss 的可修复 high；vite/esbuild 残留风险写在 research 或收尾说明里（对应 R7）。
- [x] AC8. `ci.yml` 中 rust-cache / install-action / rust-toolchain 钉到计划中的 SHA；`CI gate` 名称未改（对应 R8）。
- [x] AC9. 全量 Rust 测试与 docs 构建通过；订阅 HTTP 客户端在 reqwest 批次后仍能编译，feature 名符合 0.13（对应 R9）。
- [x] AC10. 无夹带的产品功能或无关重构（对应 R10）。

## Out of Scope

- 将 MSRV 从 1.95 升到 1.96+。
- 将开发工具链从 1.97.0 升到 1.98.0（可另开任务；本任务不改 `rust-toolchain.toml` channel）。
- syn 2 → 3（架构测试 AST walker；高 churn、无用户价值）。
- VitePress 2.0 alpha、强制 vite 6/8 override。
- CI Node 20 → 22。
- 产品侧已有废弃项：`tui` 命令别名、`PricingCatalog::static_v1()`。
- Python / 看板 JS 引入 npm/pip 依赖。
- 一次改完所有批次后只跑一次测试。

## Key Decisions

- 本任务包含：Actions SHA 刷新、Cargo lockfile+补丁对齐、docs npm override、以及最后一批 reqwest 0.13。
- 本任务不包含：工具链 1.98、syn 3、VitePress 2、MSRV 上调。
- 残留 vite/esbuild 告警视为文档开发服务器风险，等 VitePress 稳定线提供 vite ≥6.4.3 后再处理。
