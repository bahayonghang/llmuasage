# Public 只读诊断边界闭环

## Goal

完成旧 SEC-004 明确延期的 public read 边界：默认 public 模式不得向未认证远程客户端暴露原始日志、本地路径、内部诊断和 job 明细，同时保持 loopback 本地工作流。

## Confirmed Evidence

- `src/web/mod.rs:329` 仍在 public router 挂载 logs/diagnostics/job-read APIs。
- 旧 `07-24-sec-public-boundary` 的 design 将 SEC-004 延期，但父任务仍按全部整改完成归档。
- mutation route 和 500 detail 已有部分整改，本任务只处理复审仍存在的 read exposure。

## Requirements

- public 默认 router 不挂载 raw logs、内部 diagnostics、job detail/list 和含本地路径的 read routes。
- public dashboard 若依赖诊断摘要，必须使用明确 allowlist 的脱敏 DTO，不复用 loopback raw payload。
- loopback 模式保留现有日志/诊断/job-read 功能。
- 未来如需 remote diagnostics，必须使用显式 opt-in + 认证；不以 Host/Origin 作为认证。
- 错误响应和 structured logs 不得重新泄露路径、SQL 或原始日志内容。

## Acceptance Criteria

- [ ] 真实 TCP 测试中，non-loopback/public 默认访问敏感 read route 得到 404/405 或统一拒绝。
- [ ] public dashboard payload 不含绝对路径、raw JSON logs、SQL/error detail 或 job internal state。
- [ ] loopback 模式现有 logs/diagnostics/job polling tests 不回归。
- [ ] route inventory test 显式列出 public allowlist，新增敏感 route 默认失败。
- [ ] README 和中英文 docs 准确说明 public/loopback 能力差异。

## Out of Scope

- 不在本任务实现完整多用户账号系统；remote diagnostics opt-in 可后续单独设计。
