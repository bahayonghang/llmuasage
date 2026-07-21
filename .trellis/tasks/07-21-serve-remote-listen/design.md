# 设计：`serve --public` 与 SSH 浏览器策略

## Boundary

本次只改变主 Dashboard 的 CLI 启动策略。HTTP 路由、SQLite 查询、前端资源、端口候选顺序和
token-accounting 启动修复保持不变。

| 模式 | 绑定地址 | 自动打开浏览器 | 终端访问提示 |
| --- | --- | --- | --- |
| 默认本地 | `127.0.0.1` | 非 SSH 且未传 `--no-open` 时打开 | `http://127.0.0.1:<port>` |
| SSH 本地 | `127.0.0.1` | 跳过 | 本地 URL 加 SSH 隧道命令模板 |
| `--public` | `0.0.0.0` | 非 SSH 且未传 `--no-open` 时打开本地 loopback URL | `http://<server-host-or-ip>:<port>` 加安全警告 |
| `--no-open` | 由 `--public` 决定 | 跳过 | 保留相应模式的访问提示 |

## CLI Contract

`Commands::Serve` 新增两个布尔字段：

- `--public`: 显式将监听 IP 切换到 `0.0.0.0`。
- `--no-open`: 不尝试启动默认浏览器。

无参数行为保持为 `--public=false`、`--no-open=false`。`--public` 仅说明网络监听，不能暗示
Dashboard 自带认证、TLS 或安全的公网部署能力。

## Server Boundary

为避免破坏可能直接调用 `web::serve(store, port)` 的嵌入方，保留其现有 public 签名和 loopback
语义。把现有实现提取到 crate-internal helper，由命令层传入具体 `IpAddr`；public wrapper 固定传入
`127.0.0.1`。helper 保留指定端口或 `37421/37422/37423/0` 的端口探测行为，并返回实际 listener
地址供 run log 使用。

## URL And Output Contract

`0.0.0.0` 是通配绑定地址，不能作为用户应在浏览器输入的 URL。启动逻辑按实际端口构造：

- 本机 URL 始终为 `http://127.0.0.1:<port>`，仅用于本机提示和浏览器启动。
- 公开模式打印 `http://<server-host-or-ip>:<port>` 模板，并明确由用户替换为服务器可路由的
  IP 或主机名。
- SSH 默认模式打印端口转发模板 `ssh -L <port>:127.0.0.1:<port> <user>@<server>`，不臆测
  用户名或主机名。
- 公开模式额外输出无认证/TLS 风险及防火墙、SSH 隧道或反向代理的保护建议。

## Browser Decision

将浏览器启动资格收敛为可单测的纯逻辑：`--no-open` 优先关闭；否则当 `SSH_CONNECTION` 或
`SSH_TTY` 任一非空时跳过；其余情况保持现有 launcher。仅真正应启动时才调用 `xdg-open` / `open` /
`cmd start`，因此 SSH 环境不会产生 launcher failure warning。非 SSH 环境的 launcher 真实失败仍
保留现有 warning，避免掩盖本地故障。

## Documentation And Security

更新顶层 help、双语 README、CLI reference、Dashboard、Getting Started、Safety 和 Architecture
的中英文对应页面。所有原先绝对化的“只绑定 `127.0.0.1`”表述改为“默认只绑定”，并把公开模式的
安全限制放在 Safety 和 Dashboard 页面。`codex-tracer` 文档不在本次范围内。

## Compatibility And Rollback

不传新参数的 CLI、`web::serve` public API、默认 URL、端口顺序和本地自动打开行为均不变。回滚只需
删除新的 flags、internal bind helper 调用和文档段落；不会涉及 schema、持久化状态或数据迁移。
