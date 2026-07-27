# CI 与供应链加固（CI-001/002、SUPPLY-001/002、LEGAL-001）

## Goal

消除本地/CI gate 漂移，补齐平台与并发覆盖，固化构建可重复性与更新信任锚，修正 license 分发不一致。

## 覆盖发现（已核实）

- **CI-001（P2）**：`justfile` ci 目标跑 4 个 dashboard JS tests（dashboard-fetch/bootstrap-watchdog/load-state/render-lifecycle）；`.github/workflows/ci.yml:84` 只跑 `dashboard-fetch.test.mjs`。watchdog/load-state/render-lifecycle 回归可合并进 main。
- **CI-002（P2）**：`.github/workflows/ci.yml:17` Rust 主 job 仅 `windows-latest`；`:72` 全部测试强制 `--test-threads=1`。Linux/macOS path/hook/permission 失败不被 gate；真实并发交错零覆盖。
- **SUPPLY-001（P2，设计风险）**：toolchain 用 moving `dtolnay/rust-toolchain@stable`（ci.yml:24,101）；third-party Actions 用 mutable tag；cargo 命令未统一 `--locked`；无 Dependabot/Renovate 配置。
- **SUPPLY-002（P2，设计风险）**：`src/commands/update.rs:157-173` self-update 执行 `cargo install --git ... --branch <channel> --locked --force`——信任锚是可变 branch HEAD，branch 被 force-push/账号被攻破即执行任意新代码；`--locked` 不固定源码 commit。
- **LEGAL-001（P2）**：`Cargo.toml:11` 声明 `MIT OR Apache-2.0`，仓库仅有 `LICENSE-MIT`，缺 Apache-2.0 文本。

## Requirements

1. GitHub Actions 直接调用 `just ci`（或单一 `scripts/ci.*`），本地与 CI gate 单一来源。
2. Windows/Ubuntu/macOS fast matrix；MSRV（1.85）job；测试默认并行，仅标注组用 nextest 串行；不允许全套 `--test-threads=1` 回退。
3. `rust-toolchain.toml` 固定版本；Actions pin SHA；cargo 统一 `--locked`；引入 Dependabot/Renovate、`cargo-deny`（advisories/licenses/bans/sources）、SBOM。
4. self-update 改为发布签名/校验和二进制，或 immutable tag+commit SHA；UI 展示将安装的 commit；dev channel 明确标注 unsafe。
5. 补 `LICENSE-APACHE` 标准文本（或经 owner 决策改 metadata 为 MIT-only）。

## Acceptance Criteria

- [ ] `just ci` 与 Actions 的命令来源只有一处；4/4 dashboard JS tests 进 CI。
- [ ] PR fast lane 三 OS 全绿；标注串行组之外的测试并行执行。
- [ ] 同一 commit 的构建输入（toolchain、Actions、deps）全部 pin。
- [ ] `llmusage update` 不再安装 branch HEAD（或明确 unsafe 标注 + 展示目标 commit）。
- [ ] `cargo package --list` 含双 license 文件，license scanner 通过。

## Notes

- CI-001 与 LEGAL-001 是审计"当天阻断项"，各 <0.5d，可先行落地。
- CI-002 展开时会暴露环境耦合 flaky——按审计要求仅隔离具体 test，不回退全套串行。
- license 选择（补 Apache vs 改 MIT-only）需 owner 决策，默认推荐补全 Apache-2.0 文本。
