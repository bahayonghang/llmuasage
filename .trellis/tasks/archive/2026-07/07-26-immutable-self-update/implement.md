# Implementation Plan: Immutable Self Update

- [x] 写 stable branch-head 旧行为失败的 planner test。
- [x] 抽取 ref resolver 和 immutable resolved target。
- [x] stable planner 改用 `--rev`，dev 保留显式 mutable warning。
- [x] 锁定 preview-confirm-execute 的同一 SHA，增加 TOCTOU 测试。
- [x] 更新 self-update contract、README 和中英文 CLI docs。
- [x] 运行 update focused tests、docs build、完整 Rust tests 与 `just ci`。

## Rollback

- resolver/planner 与文档分独立提交；解析失败始终 fail closed，不回退到 branch install。
