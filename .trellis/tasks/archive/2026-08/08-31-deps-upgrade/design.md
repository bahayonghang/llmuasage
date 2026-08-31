# 分批升级设计

## Architecture / boundaries

四个可写表面，互不混批：

1. `.github/workflows/ci.yml` — Actions SHA。不改 job 图、不改 `CI gate` 名称、不改 MSRV 版本字符串。
2. `Cargo.lock` + 对齐用的 `Cargo.toml` 补丁号 — 只吸收 `cargo update` 在 MSRV 1.95 约束下允许的版本。
3. `docs/package.json` overrides + `docs/package-lock.json` — 只钉 nanoid/postcss 的已发布补丁。
4. `Cargo.toml` reqwest 声明 + 若编译器要求的调用点 — 0.12 → 0.13，独立一批。

产品代码（parsers/store/query/web/tui）不应在前三批出现 diff。第四批最多改 `Cargo.toml` feature 名；`src/subscription/http.rs` 当前只用 `Client::builder` / `StatusCode` / `json()`，预期无需逻辑改写。

## Compatibility

- MSRV 1.95 保持权威。`cargo update` 已证明 lockfile 刷新会 “Locking … to latest Rust 1.95 compatible versions”。
- 开发工具链继续 1.97.0；`just install` 的 `cargo +1.97.0` 不改。
- axum 仍停在 0.8.9（0.9 未发布）。tower-http 直接依赖已是 0.7.0；axum 传递 0.6.11 允许并存。
- sha2 直接依赖已是 0.11.0；树中残留 sha2 0.10.9 为传递项，不手工 duplicate 消除。

## reqwest 0.13 contract

见 `research/reqwest-0.13-and-syn-3.md`。

```toml
reqwest = { version = "0.13", default-features = false, features = [
    "rustls",
    "json",
    "http2",
] }
```

默认 crypto 从 ring 切到 aws-lc。必须在 Windows/Linux/macOS 的 Rust CI 矩阵上验证（`just ci` 的本地 Windows 通过后，仍依赖 GitHub `rust` matrix）。不改订阅 URL、超时、User-Agent、错误文案。

## npm overrides

```json
"overrides": {
  "nanoid": "3.3.18",
  "postcss": "8.5.26"
}
```

不 override `vite`/`esbuild`：vitepress 1.6.4 声明 `vite ^5.4.14`，而相关 GHSA 覆盖整个 ≤6.4.2，5.4.21 已是 5.x 终点。强行拉 vite 6+ 属于 Breaking，且 VitePress 2 仍是 alpha。

## Actions pins (target)

| Action | Target |
| --- | --- |
| Swatinem/rust-cache | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` # v2.9.2 |
| taiki-e/install-action | `1ed6d7be6168f6c9046541087ff549b6bc581fdf` # v2.87.2 |
| dtolnay/rust-toolchain | `6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772` |
| checkout / setup-node | 保持现钉 |

v2.9.2 含 Cargo V2 build dir 清理，与 1.97 兼容；预期只影响缓存命中，不改变编译结果。

## Rollout / rollback

- 每批一个逻辑提交（实施阶段再按仓库 Conventional Commits 规范拟文）。
- 回滚：`git restore` 该批文件，或 `git revert` 该批提交。lockfile 批次禁止手工编辑 `Cargo.lock` 条目。
- 若 rusqlite 0.40.2 / libsqlite3-sys 0.38.2 在 MSRV 上失败：整批 lockfile 回滚，不得只钉 sqlite 而留下半更新树。
- 若 reqwest 0.13 在某一 OS 因 aws-lc 构建失败：回滚 reqwest 批次，保留前几批。

## Trade-offs

- 接受 vite/esbuild 残留告警，避免把文档站绑到 alpha VitePress。
- 接受 syn 2 与传递 syn 3 并存，避免改架构测试访问器。
- 把 reqwest 放在最后一批：它是唯一需要改 manifest feature 且可能改变 native TLS 构建图的产品依赖。
