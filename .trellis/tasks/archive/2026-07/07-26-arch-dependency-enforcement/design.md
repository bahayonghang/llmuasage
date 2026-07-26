# Design: Architecture Dependency Enforcement

## Dependency Direction

```text
commands / web adapters -> application sync service -> domain/store/parser ports
```

- application 定义 executor/service trait 与 typed request/result。
- command-specific executor 留在 adapter/composition root。
- JobRegistry 接受 injected `Arc<dyn SyncExecutor>` 或项目现有等价泛型 seam。

## Enforcement

- 首选解析 Rust module/use/path 的项目脚本或现有 architecture-test crate；输入固定为 `src/sync` 等受限层。
- gate 输出违规源文件、line 和 dependency target。
- fixture tests 覆盖全限定路径，避免当前 grep 再次误判完成。

## Compatibility

- 不改变 JSON event、JobSnapshot、CLI output 或 Web routes。
- 只移动 construction ownership，保持 executor 行为。
