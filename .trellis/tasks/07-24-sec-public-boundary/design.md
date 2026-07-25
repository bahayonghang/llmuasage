# design: public 模式安全边界修复（SEC-001/003/004）

## 根因分析

### SEC-001

- `bind_server` 不接受 `public` 标志，mutation 路由在所有模式下挂载
- `reject_non_local_write` 读取客户端可控的 `Host` 头而非真实 peer IP
- `Origin` 缺失时直接放行（攻击向量：`curl -X POST -H 'Host: localhost:PORT' http://<server>:PORT/api/jobs`）

### SEC-003

- `api_diagnostics_forget` 500 响应含 `detail: err.to_string()`（SQLite 错误/文件路径可泄漏）
- `api_json` wrapper 在所有 handler 失败时同样泄漏

### SEC-004

- `--public` 下日志 API 可返回原始 JSON（含绝对路径），diagnostics 含文件路径，无脱敏

## 修复策略

### SEC-001（Router 层分离 + ConnectInfo 验证）

**方案**：两层防御

1. `WriteExposure::PublicReadOnly`：Router 层面不挂载 mutation 路由（最强防线）
2. `WriteExposure::LocalOnly`：挂载 mutation 路由，但验证 `ConnectInfo<SocketAddr>` 真实 peer IP

```
WriteExposure::LocalOnly    → 挂载 mutation 路由，peer.ip().is_loopback() 检查
WriteExposure::PublicReadOnly → 不挂载 mutation 路由（404/405 自然响应）
```

**枚举**：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WriteExposure {
    /// Mutation routes accessible only from loopback peers (default: loopback bind).
    LocalOnly,
    /// Mutation routes not mounted; server binds to 0.0.0.0 (--public).
    PublicReadOnly,
}
```

**变更点**：

1. `bind_server(store, port, bind_ip, write_exposure: WriteExposure)` 新增参数
2. `serve_on` → 传 `WriteExposure::LocalOnly`（已是 loopback 绑定）
3. Router 条件挂载 mutation 路由
4. `axum::serve(...).into_make_service_with_connect_info::<SocketAddr>()`
5. 替换 `reject_non_local_write(&HeaderMap)` → `reject_non_local_write(SocketAddr)`
6. mutation handlers 改为提取 `ConnectInfo<SocketAddr>` 替代 `HeaderMap`

### SEC-003（500 响应脱敏）

- `api_json` / `api_diagnostics_forget` 的 500 响应移除 `"detail": err.to_string()`
- 完整错误链保留在 `error!()` structured log 里
- 保留 `"code"` + `"message"` + `"endpoint"`（固定字符串，不泄漏内部状态）

### SEC-004（范围约束，暂不做 read API 完全屏蔽）

本任务仅做最关键一步：`--public` 模式中 mutation 路由不存在。
read API 的路径脱敏（logs/diagnostics 中绝对路径）属于长期 P2，记录在 Notes 但不在本次 DoD 内。
原因：路径脱敏需要 query layer 改动，不宜与 P0 修复合并。

## 接口变更

| 函数                     | before                             | after                                        |
| ------------------------ | ---------------------------------- | -------------------------------------------- |
| `bind_server`            | `(store, port, bind_ip)`           | `(store, port, bind_ip, write_exposure)`     |
| `serve_on`               | `(store, port, bind_ip)`           | 内部传 `LocalOnly`，签名不变                 |
| `reject_non_local_write` | `(&HeaderMap) -> Option<Response>` | `(SocketAddr) -> Option<Response>`           |
| mutation handlers        | `headers: HeaderMap`               | `ConnectInfo(peer): ConnectInfo<SocketAddr>` |

## 测试

1. **regression（先写、先红）**：`public_write_routes_rejected_via_real_tcp`
   - 绑定 `WriteExposure::PublicReadOnly` 服务
   - 从 loopback 发送 `POST /api/jobs`、`POST /api/jobs/x/cancel`、`POST /api/diagnostics/forget`
   - 断言 404 或 405（路由不存在）

2. **loopback 不回归**：`local_write_routes_accessible_from_loopback`
   - `WriteExposure::LocalOnly` + loopback peer → mutation 路由正常响应

3. **单元测试**：`reject_non_local_write` 直接测 loopback vs 外部 IP

4. **detail 脱敏**：注入故意失败路径，确认 500 响应体不含内部错误字符串

## 不改动

- 现有 `route_json_with_headers` 测试工具函数（无需改）
- loopback 模式下的写路由行为（API 语义不变）
- `serve_on` / `serve` 的公开签名（内部传 LocalOnly）
