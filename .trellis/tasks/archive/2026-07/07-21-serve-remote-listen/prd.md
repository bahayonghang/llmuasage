# 优化 serve 远程监听与 SSH 启动

## Goal

让 `llmusage serve` 能在远程服务器上以明确、可控的方式提供 Dashboard，同时避免 SSH
会话中无可用图形浏览器时出现无意义的浏览器启动告警。公开监听使用简短的显式 opt-in
参数 `--public`，不改变无参数启动的本地安全默认值。

## Confirmed Facts

- `Commands::Serve` 目前只接受可选的 `--port`（`src/commands/mod.rs:133`），并将其传给
  `commands::serve::run`（`src/commands/mod.rs:301`）。
- Web 服务在 `web::serve` 中把监听地址硬编码为 `127.0.0.1`，端口顺序为指定端口或
  `37421/37422/37423/0`（`src/web/mod.rs:70`、`src/web/mod.rs:126`、`src/web/mod.rs:134`）。
- 服务启动后总会尝试打开浏览器；Unix 使用 `xdg-open`，其非零退出会作为 warning 写入日志，
  但服务会继续运行（`src/commands/serve.rs:48`、`src/commands/serve.rs:133`、
  `src/commands/serve.rs:165`）。用户在 SSH 服务器上观察到了该路径的 `exit status: 3`。
- `codex-tracer` 已提供 `--no-open` 布尔参数及对应 dispatch 方式，可作为主 Dashboard
  命令的一致性先例（`src/commands/mod.rs:152`、`src/commands/mod.rs:308`）。
- 当前文档和安全边界将默认行为描述为仅监听 `127.0.0.1`（例如
  `docs/safety/index.md:102`、`docs/reference/cli.md:248`）。默认行为必须保持该私有边界，
  不能把已有本地安装静默暴露到网络。

## Requirements

- R1: `serve` 必须提供布尔参数 `--public`；指定后监听 `0.0.0.0`，使远程浏览器、反向代理或端口映射可以访问 Dashboard。不得新增需输入 IP 的 `--host` 参数。
- R2: `serve` 必须提供 `--no-open`，显式关闭自动打开浏览器；未指定新参数时，监听地址、端口探测顺序和浏览器自动打开的本地行为保持兼容。
- R3: 检测到 SSH 会话（非空 `SSH_CONNECTION` 或 `SSH_TTY`）时，除非未来有明确的图形转发需求，否则不得调用浏览器启动器或记录原示例中的失败 warning；终端仍须打印准确的访问提示。
- R4: 对 `--public`，启动输出必须区分监听地址和可导航地址：`0.0.0.0` 仅是绑定地址，远程访问说明使用服务器实际 IP/主机名占位符；本机自动打开时使用 `127.0.0.1`。
- R5: 使用 `--public` 时，输出和文档必须明确 Dashboard/API 暴露本地使用数据，且不提供认证或 TLS；用户需借助防火墙、SSH 隧道或反向代理保护访问。
- R6: 新参数、启动输出和中英文文档必须一致；为 CLI 参数、绑定行为和 SSH 浏览器策略添加聚焦回归测试。

## Acceptance Criteria

- [x] `llmusage serve --public --port <free-port>` 成功监听 `0.0.0.0:<free-port>`，并保留现有 HTTP 路由；`llmusage serve --port <free-port>` 仍只监听 `127.0.0.1:<free-port>`。
- [x] 省略 `--port` 时仍按 `37421/37422/37423/0` 的既有顺序探测；公开与本地模式使用同一顺序。
- [x] SSH 环境不会调用浏览器启动器或产生原示例中的失败 warning；输出在本地模式给出 SSH 隧道提示，在公开模式说明以服务器 IP/主机名和端口访问。
- [x] 非 SSH 本地环境仍默认尝试打开浏览器；`--no-open` 关闭该行为。`--public` 的本地浏览器 URL 为 `127.0.0.1:<port>`，不会把 `0.0.0.0` 作为浏览 URL。
- [x] CLI help、README 和相关中英文文档准确说明 `--public`、`--no-open`、默认 loopback、SSH 用法及无认证/TLS 的暴露风险。
- [x] 聚焦 Rust 测试、格式化和相关静态检查通过；最终按风险决定是否运行完整 `just ci`。

## Notes

- 预期实现范围仅限主 Dashboard 的 `serve` 命令，不扩展到 `codex-tracer`，也不添加认证、TLS、反向代理配置、IPv6 或任意自定义绑定地址。
