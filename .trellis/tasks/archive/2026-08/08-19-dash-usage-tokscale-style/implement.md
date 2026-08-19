# Implement: Dash Usage 对齐 tokscale 订阅额度页

## Checklist

1. **依赖**
   - `Cargo.toml` 增加 `reqwest`：`default-features = false`，`features = ["rustls-tls", "json", "http2"]`。
   - `cargo update -p reqwest` 写入 lockfile。
   - 不改 `rust-version`，除非 MSRV check 失败。

2. **subscription 模块**
   - 新增 `src/subscription/{mod,types,cache,claude,codex,grok,kimi}.rs`。
   - `lib.rs` `pub mod subscription;`，不放进根 façade re-export。
   - `fetch_all(endpoints, intent)` → `UsageFetchReport`。intent 只有 TUI 只读。
   - 缓存读写：`AppPaths` 增加 `cache_dir` + `subscription_cache_path`，TTL 300s。
   - 每提供者 `has_credentials` / `fetch`。禁止写凭证路径。

3. **fetcher 测试**
   - 本地 HTTP 夹具（axum 或 TcpListener）。覆盖：成功解析、HTTP 4xx 诊断、超时、凭证缺失跳过、失败后凭证字节不变、缓存命中不发请求。
   - 测试根目录用 tempfile，不读真实 `~/.claude` / `~/.grok`。

4. **QuotaController + TUI state**
   - `src/tui/quota.rs`：后台任务 + `try_recv`，对标 `SyncController`。
   - `AppState`：`quota_report`、`hide_usage_emails`（默认 true）、`quota_fetch_attempted`。
   - 进入 Usage：`fetch_if_needed`。`r`：`force_fetch`。`R` / `needs_refresh`：不调用 controller。
   - `update_scroll_total(Trends)` 改为账号行数。
   - `panel_has_data(Trends)`：额度已尝试过（成功、空、失败都算）即可画主区域；overlay 仍看 `sync_center`。

5. **Usage 主面板**
   - 重写 `src/tui/panels/usage.rs` 为额度构图（宽/中/窄）。
   - 现有 sync 渲染挪到 `src/tui/panels/sync_status.rs`（或 `sync_overlay.rs`）。
   - 进度条、Health 色走 theme accessor。禁止 `Color::*`。

6. **overlay 与按键**
   - `ActiveDialog::SyncStatus`。`y` → 打开。Esc/`q` 关 overlay。
   - overlay 打开时 `j/k` 滚 Source Sync；`x` 仍到 `StartSync`。
   - `m` → 切换邮箱。Help / footer 补 `y` / `m`。
   - `data_loader` 的 `Panel::Trends` 继续加载 `sync_command_center`。

7. **文档与契约**
   - `README.md`、`README.zh-CN.md`：Usage 会用本机 CLI 凭证读订阅额度。
   - `tui-presentation-contracts.md`：Usage 主区域是额度页；sync 计数只出现在 overlay。
   - `tui-runtime-contracts.md`：Usage 可滚动账号行；`R` 不拉额度；overlay 是第三种 dialog。
   - 新增 `tui-subscription-contracts.md` 或写入 backend index：只读凭证、5 分钟缓存、四家提供者、测试禁公网。

8. **TUI 测试**
   - 改 `tests/tui_panels_prop.rs`：主区域断言新标题/列；overlay 断言旧 Source Sync 文案。
   - NoColor、窄宽、隐藏邮箱、诊断行、空态。
   - 主题源码守卫包含新文件。

9. **Gate**
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - subscription + TUI 相关测试，再 `python scripts/ci-rust.py` 或全量。

## Validation

```text
cargo test --lib subscription -- --test-threads=1
cargo test --test tui_panels_prop -- --test-threads=1
cargo test --lib tui -- --test-threads=1
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

实现后用 `llmusage dash` 打开 Usage：有凭证则见 Accounts；按 `y` 仍能看见 Source Sync；`x` 仍能 sync。

## Risky files

| 文件 | 风险 |
| --- | --- |
| `Cargo.toml` / `Cargo.lock` | reqwest 特征过大或拖进 native-tls |
| `src/subscription/*.rs` | 误写凭证；测试打到公网 |
| `src/tui/mod.rs` | `R` 误走 force_fetch；`r` 漏走 force_fetch |
| `src/tui/input.rs` | `y`/`m` 与 dialog 抢键 |
| `src/tui/panels/usage.rs` | 直接写 `Color::*` |
| `tests/tui_panels_prop.rs` | 旧 `Usage / Sync` 主区域断言会红 |
| README | 漏改中文页，产品承诺不一致 |

## Rollback

无 migration。还原 TUI Usage、删除 `src/subscription` 与 `reqwest`、还原 README。可保留空的 `~/.llmusage/cache/`。

## Before start

- 用户已批准本规划摘要。
- 先读 `tui-presentation-contracts.md`、`tui-runtime-contracts.md`、`ci-toolchain-contracts.md`、`research/tokscale-usage-gap.md`。
- 对照 tokscale 四家 fetcher，不要复制写凭证路径。
- 不要开 web 同步页、Amp、Add Codex。
