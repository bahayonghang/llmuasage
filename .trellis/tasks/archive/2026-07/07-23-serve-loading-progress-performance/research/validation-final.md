# 最终验证记录（2026-07-23）

## 证据边界

- 只记录脱敏时序、字节数、请求计数、DOM 状态、support level 与测试结果。
- 未保存 API body、HAR、截图、项目路径、prompt、session 或原始 usage event。
- 用户原页面对应的进程已退出是确认事实；导致该进程退出的精确触发源仍为 `unknown`。

## 服务生命周期

- CLI 监督 `BoundWebServer`，在 Ctrl+C 与 server task 之间选择；task error/panic 可观察，graceful shutdown 上限为 3 秒。
- 显式端口冲突返回包含地址和 bind 原因的结构化错误，不再 panic。
- `run_log` 覆盖完整 serve 会话；新 serve 会把遗留的 stale `running` 恢复为 `aborted`。受控验证中，旧记录 133/134/135 已恢复，当前试用服务记录 136 保持 `running`。
- 最终试用服务仍监听 `http://127.0.0.1:37424`，根路由返回 HTTP 200。

## 前端状态与故障矩阵

| 场景 | 结果 |
| --- | --- |
| 正常首屏 | 1 个 interactive core 请求后立即渲染，随后 5 个 secondary；无 full 请求和 13 endpoint fallback |
| `app.js` 或依赖被阻断 | watchdog 结束静态 loading，显示资源启动失败并提供 Reload |
| ready 前异常/rejection | watchdog 进入 error，静态 loading 被替换 |
| API HTTP/network/parse failure | 每轮最多 1 个 dashboard 请求；不调用 legacy fallback 或 `/api/jobs` |
| Core 慢 | 2 秒进入 slow；6 秒 abort/timeout 并提供 Retry |
| Retry | 创建新 generation/controller；键盘操作有效，旧响应不能覆盖 |
| 服务停止 | 有界根探测显示本地服务不可用 |
| Snapshot module failure | 不探测本地服务，显示离线资源错误 |
| Secondary | 并发上限 2；可见进度 `[0,1,2,3,4,5]`；fulfilled/degraded 均推进一次 |

## 性能预算

| 范围 | Interactive HTTP p95 | Payload |
| --- | ---: | ---: |
| 1d | 12.40 ms | 15,026 B |
| 7d | 14.89 ms | 16,558 B |
| 30d | 34.27 ms | 27,551 B |
| all | 43.17 ms | 59,075 B |

- 浏览器核心渲染最大 59.3 ms，五个 secondary 全部 settle 最大 2,105.7 ms，交互反馈最大 3.2 ms。
- 均满足 interactive p95 <= 400 ms、payload <= 128 KiB、核心渲染 <= 1,000 ms、secondary 完成 <= 4,000 ms。
- 代表库 support level：activity `degraded`、tools `degraded`、optimize `degraded`、explorer `normalized`、compare `degraded`。

## 客户端过滤器兼容性

- 用户原 Chrome 扩展环境明确报告 `/assets/data/fingerprint.js` 为 `net::ERR_BLOCKED_BY_CLIENT`。该路径是 `app.js` 顶层 ES module 依赖，因此单一资产被客户端拦截会阻止整个 module graph 执行；这解释了服务端和 API 可用但页面仍无法进入应用加载状态的差异。
- 资产文件、embedded registry 与所有 import 已统一改为 `/assets/data/render-key.js`。helper 的 `stableSerialize` / `panelFingerprint` 导出和 render cache key 行为未变；旧路由明确返回 404。
- focused 验证：render lifecycle Node tests 13/13；Rust asset tests 16/16；新路由 200 且 body 正确，旧路由 404，live module graph 不含 `fingerprint` URL。
- 同一用户 Chrome 扩展环境复验：新资产 HTTP 200；含 `fingerprint` 的请求 0；`Network.loadingFailed` 0；runtime exception 0；dashboard 请求 1；bootstrap/loading error false；页面完成渲染。
- 本轮没有保存 HAR、截图、API body、数据库或 usage artifact。

## 视觉与可访问性

- 验证视口：320x720、720x900、1440x900、1920x1080；页面宽度等于 viewport，按钮溢出为 0。
- ZH/EN、light/dark、长 finding 文本与模型表均无页面级横向溢出。
- `role=status`、`aria-live=polite`、五个 segment 的 `aria-label` 和 44px Retry target 已验证。
- reduced motion 下为 `0.01ms x 1`，50 ms 后 `transform:none`；正常 rail 动画约 1.4 秒，只使用 opacity/transform。
- Impeccable detector 仅运行一次，只报告既有/新增尺寸字面值 advisory，无阻断项。

## 自动验证

- `cargo test --lib -- --test-threads=1`：444 passed，6 ignored。
- load-state：8/8；render lifecycle + load-state：20/20。
- 最终 `just ci`：通过，包括 fmt、严格 Clippy、450 个 Rust tests、rustdoc、dashboard JS checks/tests 与 VitePress docs build。
- `git diff --check`：通过。
- 用户既有 `Cargo.toml` 1.0.2 修改保留；任务验证时曾把 CI 自动同步的 `Cargo.lock` 恢复为 1.0.1，避免混入功能修复。最终按用户“提交所有改动”的明确授权，将 `Cargo.toml` / `Cargo.lock` 的 1.0.2 版本元数据作为独立提交，不混入 serve 修复提交。
- 仓库外的独立 CI target 清理被本机执行策略拒绝；该目录未进入 Git 工作区，需后续由用户环境清理。
