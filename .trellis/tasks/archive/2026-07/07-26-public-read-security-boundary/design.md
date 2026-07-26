# Design: Public Read Security Boundary

## Router Composition

- 构建 `loopback_router` 与 `public_router` 两个明确 route inventory。
- public router 只挂载 dashboard 所需的最小 read allowlist；敏感 routes 不存在，而不是依赖请求时 header guard。
- 若需要公开 health，返回固定最小 DTO，不包含 paths/logs/jobs。

## Data Minimization

- public DTO 由专用 projection 生成，字段 allowlist 编译期可见。
- loopback DTO 可保留本地诊断能力，但错误 detail 仍遵守 generic client response 契约。

## Tests

- 使用真实 listener 和 TCP client，分别从 loopback/public composition 验证 route inventory。
- 维护 public response forbidden-key/value 断言，包括 Windows/Unix absolute path 样本。

## Compatibility

- `serve` 默认 loopback 行为不变。
- `serve --public` 收紧 read surface 是既定安全边界闭环，文档提供迁移说明。
