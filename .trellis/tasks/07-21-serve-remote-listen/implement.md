# 实施计划：`serve --public` 与 SSH 启动优化

## Steps

1. 在 `src/commands/mod.rs` 增加 `Serve` 的 `public` / `no_open` 参数、帮助文字和 dispatch 传递；在
   既有 CLI 解析测试模块中覆盖默认值及两个 flags。
2. 在 `src/web/mod.rs` 保留 public `serve(store, port)` loopback wrapper，提取接受绑定 IP 的 internal
   helper；更新 listener bind 并在 Web 测试中验证 public listener 返回 `0.0.0.0` 且经 loopback
   仍可取得既有首页路由。
3. 在 `src/commands/serve.rs` 传递 `--public` 的绑定选择，建立本地/公开 URL 与输出提示，并引入可
   单测的 SSH/`--no-open` 浏览器决策。保留普通本地浏览器启动失败的 warning。
4. 更新 `src/commands/help.rs` 的中英文顶层描述，移除“始终绑定 `127.0.0.1`”的过度承诺。
5. 更新 `README.md`、`README.zh-CN.md`，以及 `docs/{,zh/}` 下的 index、reference/cli、dashboard、
   guide/getting-started、safety、architecture 页面；给出 `llmusage serve --public --no-open --port 37421`
   示例和 SSH 隧道说明，并注明认证/TLS 缺失。
6. 运行聚焦测试与 CLI help 检查，随后运行格式、lint、完整串行 Rust 测试、文档构建和 `just ci`；检查
   final diff，记录任何 Windows binary-lock 或外部工具限制。

## Test Matrix

| Contract | Verification |
| --- | --- |
| CLI flags | `Cli::try_parse_from` 断言默认、本地、`--public` 与 `--no-open` 值 |
| Binding | Web async test 使用临时 Store 和端口 `0`，断言 public bind 地址并请求 `/` |
| Browser policy | 纯函数单测覆盖 local、SSH、`--no-open` 优先级与两个 SSH 环境变量 |
| URL semantics | 单测断言 public bind 的浏览器 URL 仍为 loopback，远程提示不把 `0.0.0.0` 当作可导航地址 |
| Docs/help | `cargo run -- serve --help` 检查 `--public` 和 `--no-open`；`npm --prefix docs run docs:build` |

## Validation Commands

```powershell
cargo test commands::tests --lib -- --test-threads=1
cargo test commands::serve::tests --lib -- --test-threads=1
cargo test web::tests --lib -- --test-threads=1
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --test-threads=1
npm --prefix docs run docs:build
just ci
git diff --check
python ./.trellis/scripts/task.py validate 07-21-serve-remote-listen
```

If a Windows process holds the default binary during the broad gate, rerun the affected command with an isolated
temporary `CARGO_TARGET_DIR`; do not treat that environmental failure as a product-test pass.

## Review Gates

- Before `task.py start`: verify all PRD criteria map to a step and no public default was introduced.
- Before completion: inspect exact documentation claims and confirm `0.0.0.0` appears only as a bind address,
  never as the recommended browser URL.
