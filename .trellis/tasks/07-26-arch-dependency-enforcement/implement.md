# Implementation Plan: Architecture Dependency Enforcement

- [x] 先添加能捕获当前全限定引用的失败 architecture test。
- [x] 将 executor trait/request/result 放到 application/sync owning layer。
- [x] 由 CLI/Web/TUI composition root 注入 concrete executor。
- [x] 删除 sync 层的 command construction 与引用。
- [x] 用多语法 fixtures 替换单一 grep gate并接入 `just ci`/Actions。
- [x] 运行 architecture、JobRegistry、public API、CLI/Web/TUI lifecycle focused tests。
- [x] 由主会话运行最终 `python scripts/ci-rust.py` 与 `just ci`。

## Rollback

- service seam 与 CI gate 分开提交；gate 先以测试证明能力，再设为 required。
