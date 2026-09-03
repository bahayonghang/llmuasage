# Journal - lyh (Part 2)

> Continuation from `journal-1.md` (archived at ~2000 lines)
> Started: 2026-09-03

---



## Session 68: 依赖扫描与分批升级

**Date**: 2026-09-03
**Task**: 依赖扫描与分批升级
**Branch**: `dev`

### Summary

09-03-deps-upgrade：扫描后按风险分批落地。Batch 0 将 taiki-e/install-action 钉到 v2.87.4。Batch 1 在 MSRV 1.95 下 cargo update 并对齐 tower-http 0.7.1（含 aws-lc-sys 0.45）。两批 just ci 均为 0。未改 syn 3、工具链 1.98、VitePress 2。

### Git Commits

| Hash | Message |
|------|---------|
| `437e62f` | (see git log) |
| `cd1b433` | (see git log) |
| `95c4288` | (see git log) |

### Status

[OK] **Completed**


## Session 69: 按业务风险补齐核心测试覆盖

**Date**: 2026-09-03
**Task**: 按业务风险补齐核心测试覆盖
**Branch**: `dev`

### Summary

静态盘点 933 个 Rust 测试后，按风险补齐 write_transaction 回滚、reset 保留运维表、cursor JSON、query/logs 边界、public host 剥离、forget/explorer 400、sync/remote/subscription 校验。全量 1030 passed。dashboard 未知 source/window/timezone 静默退化未改。

### Git Commits

| Hash | Message |
|------|---------|
| `b5234b4` | (see git log) |
| `c48f8a9` | (see git log) |
| `010f4dc` | (see git log) |

### Status

[OK] **Completed**
