# 分批升级设计（2026-09-03）

## Architecture / boundaries

两个可写表面，互不混批。docs npm 本轮无兼容修复可做，不设批次。

1. `.github/workflows/ci.yml` — 只改 `taiki-e/install-action` 的 `uses:` SHA 与版本注释。不改 job 图、不改 `CI gate` 名称、不改 MSRV / 开发工具链字符串、不改 Node 20。
2. `Cargo.lock` + 对齐用的 `Cargo.toml` `tower-http` 补丁号 — 只吸收 `cargo update` 在 MSRV 1.95 约束下允许的版本。

产品代码（parsers/store/query/web/tui）不应出现 diff。`src/web/mod.rs` 继续只用 `CompressionLayer`；0.7.1 的 `ServeDir` 行为变化不进入本仓库调用面。

## Compatibility

- MSRV 1.95 保持权威。`cargo update --dry-run` 已证明会 “Locking 10 packages to latest Rust 1.95 compatible versions”。
- 开发工具链继续 1.97.0；`just install` 的 `cargo +1.97.0` 不改。
- axum 仍停在 0.8.9（0.9 未发布）。tower-http 直接依赖升到 0.7.1；axum 传递 0.6.11 允许并存。
- reqwest 保持 0.13.4，feature 仍为 `rustls` + `json` + `http2` + `default-features = false`。
- syn 直接依赖保持 2.x。传递 syn 3.0.4 已在 lock 中，允许并存。

## Cargo lockfile batch contents

`cargo update` 将移动：

- 直接：`tower-http` 0.7.0 → 0.7.1（随后把 `Cargo.toml` 写成 `0.7.1`）
- 传递补丁：async-compression、aws-lc-rs、compression-codecs、compression-core、libredox、lru 0.18.4、mio
- 传递 1.x minor：`smallvec` 1.15.2 → 1.16.0
- 传递 0.x：`aws-lc-sys` 0.44.0 → 0.45.0（native TLS 构建图；回滚触发点）

禁止手工编辑 `Cargo.lock` 以排除 `aws-lc-sys` 0.45。

## Actions pin (target)

| Action | Target |
| --- | --- |
| taiki-e/install-action | `e67fa11c4b9316fa714ddf0abed07a0c3143b95b` # v2.87.4 |

checkout / setup-node / rust-cache / rust-toolchain 保持现钉。

## npm

无本轮变更。overrides 保持：

```json
"overrides": {
  "nanoid": "3.3.18",
  "postcss": "8.5.26"
}
```

## Rollout / rollback

- 每批一个逻辑提交（实施阶段再按仓库 Conventional Commits 规范拟文）。
- 回滚：`git restore` 该批文件。lockfile 批次禁止手工编辑 `Cargo.lock` 条目。
- 若 `aws-lc-sys` 0.45 在某一 OS 构建失败：整批还原 `Cargo.toml` + `Cargo.lock`，保留 Actions 批次。
- 若 `tower-http` 0.7.1 导致压缩协商测试失败：同样整批回滚 Cargo，不单独钉 0.7.0 而留下半更新树。

## Trade-offs

- 接受 vite/esbuild 残留告警，避免把文档站绑到 alpha VitePress。
- 接受 syn 2 与传递 syn 3 并存，避免改架构测试访问器。
- 接受 `aws-lc-sys` 0.45 随 lockfile 批次进入：拆出去需要手工拼锁，违反 ci-toolchain-contracts。
