> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# A：整合全部语言交付的双语文档与规范，证明CLI/TUI一致、机器消费者稳定，并提供真实终端证据与最终门禁。

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation, 10-20-cli-language-sync-console, 10-20-cli-language-command-output, 10-20-tui-language-consistency。Trellis父子关系不自动调度这些依赖。

## 目标与范围

整合全部语言交付的双语文档与规范，证明CLI/TUI一致、机器消费者稳定，并提供真实终端证据与最终门禁。

R9与R1-R11整体验收；父AC1-AC16逐条闭环。

## 文件责任

- `README.md`
- `README.zh-CN.md`
- `docs/reference/cli.md`
- `docs/zh/reference/cli.md`
- `docs/commands/index.md`
- `docs/zh/commands/index.md`
- `docs/guide/first-sync.md`
- `docs/zh/guide/first-sync.md`
- `.trellis/spec/llmusage/backend/runtime-log-contracts.md`
- `.trellis/spec/llmusage/backend/source-sync-contracts.md`
- `.trellis/spec/llmusage/backend/report-cli-contracts.md`
- `.trellis/spec/llmusage/backend/tui-presentation-contracts.md`
- `.trellis/spec/llmusage/backend/tui-runtime-contracts.md`
- `.trellis/spec/llmusage/backend/tui-subscription-contracts.md`
- `.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output/research/`
- `tests/cli/language.rs`
- `tests/tui/shell.rs`

本子任务拥有文档/规范一致性和跨面测试组织，不顺手修产品缺陷；缺陷退回F/S/C/T所有者。仓库没有独立terminal-dashboard页面，TUI说明写入已有CLI reference的dash/TUI章节，不新增重复主题页。

## 验收标准

- [ ] A-AC1：父AC1-AC16都有owner和证据，无unowned C/T覆盖项；产品失败没有被文档或fallback掩盖。
- [ ] A-AC2：双语README/CLI/first-sync/TUI文档与实际命令一致；新设置范围、en默认、TUI启动生效和机器/历史例外可查。
- [ ] A-AC3：相关spec不再同时要求TUI English-only；privatecache可选metadata/日志id与公开协议边界清晰，Web/desktop无越界改动。
- [ ] A-AC4：just ci完整通过且不改lockfile；定向cli/sync/tui/remote/architecture与logging/subscription回归证据可定位。
- [ ] A-AC5：普通窗口真实CLI/TUI英中smoke有独立证据；未跑项NOT_RUN，TestBackend/编译不被宣称nativePASS。
- [ ] A-AC6：组合diff仅授权范围，无真实用户usage/SQLite/凭据/日志复制，无安装/版本提升/提交/推送/发布副作用。

## 范围外与风险

最危险是把自动检查/编译当native证据，或为了让CI绿改协议/lockfile。native不可用要如实保留NOT_RUN；发现产品缺陷回owner，不能打补丁绕过契约。

不新增第三语言/热切换，不修改Web/desktop语言、token/query/sync算法，不操作真实usage或凭据，不安装/提交/推送/发布。没有阻塞产品决策；技术机制见design，执行前仍需用户明确解除不实现限制。
