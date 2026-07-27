# Implementation Plan: Public Read Security Boundary

- [x] 建立当前 public 敏感 routes 可访问的真实 TCP 失败测试。
- [x] 拆分 router composition，并定义 public read allowlist。
- [x] 为必要 public payload 建立脱敏 DTO projection。
- [x] 增加 route inventory 和 forbidden-field contract tests。
- [x] 验证 loopback logs/diagnostics/job polling 全部保留。
- [x] 更新 web-server contract、README 和中英文 docs。
- [x] 运行 web/security tests、完整 `web::tests`、strict Clippy、dashboard JS tests 与 docs build。
- [x] 归档前由主会话运行完整 Rust gate 与 `just ci`。

## Rollback

- router composition 和 DTO projection 独立提交；不得回退到 header-only 保护。
