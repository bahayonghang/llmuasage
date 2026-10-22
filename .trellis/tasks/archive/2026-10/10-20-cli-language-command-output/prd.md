> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# C：按冻结命令树把其余CLI结果、自有错误/交互和报告说明全量迁移到共享语言设置，保护所有机器数据与命令行为。

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation, 10-20-cli-language-sync-console。Trellis父子关系不自动调度这些依赖。

## 目标与范围

按冻结命令树把其余CLI结果、自有错误/交互和报告说明全量迁移到共享语言设置，保护所有机器数据与命令行为。

R3、R8；父AC4/AC9/AC11/AC15；surface清单C03-C06、C08-C21运行期输出（C07归S；帮助/parse归F）。

## 文件责任

- `src/commands/blocks.rs`
- `src/commands/catalog.rs`
- `src/commands/clean.rs`
- `src/commands/codex_tracer/`
- `src/commands/daily.rs`
- `src/commands/dash.rs`
- `src/commands/diagnostics.rs`
- `src/commands/doctor.rs`
- `src/commands/export.rs`
- `src/commands/focused.rs`
- `src/commands/init.rs`
- `src/commands/logs.rs`
- `src/commands/monthly.rs`
- `src/commands/remote.rs`
- `src/commands/report_args.rs`
- `src/commands/serve.rs`
- `src/commands/session.rs`
- `src/commands/source_status.rs`
- `src/commands/status.rs`
- `src/commands/statusline.rs`
- `src/commands/unified_report.rs`
- `src/commands/uninstall.rs`
- `src/commands/update.rs`
- `src/commands/weekly.rs`
- `src/tui/report_table.rs`
- `src/common/i18n/commands.rs`
- `tests/cli/`
- `tests/remote/`

codex_tracer范围只限.rs终端外壳，不改dashboardHTML/JS/翻译。共享domain/cleanup状态仅在确需中立typed诊断时补private反馈/分类，维持public字段/持久化原值；禁止解析自由文本猜状态。

## 验收标准

- [ ] C-AC1：C03-C21各适用success/empty/rejected/failure在en/zh配对，20focused路径从schema展开核对，没有新的自有英文/中文漏进raw fallback。
- [ ] C-AC2：CLI报告表中文对齐，原总计/成本/缓存占位/units及source排序不变；全部报告JSON固定数据和值不随lang或--locale变化。
- [ ] C-AC3：statusline单行、hook JSON读取和成本策略不变；语言cache互相隔离，无另一语言/旧latest回放。
- [ ] C-AC4：diagnostics/stdout与--out、logs/doctor/catalog JSON、remote handshake等保持原字段；英文/中文错误仍同exitcode。
- [ ] C-AC5：update各确认/EOF/target变化/failure路径用fake provider/executor验证，拒绝前零执行；remote/clean/uninstall删除集合与默认值不变。
- [ ] C-AC6：logs新消息重放统一语言，旧记录注明原文且不改磁盘；source/host/quality/accounting/cleanup状态根据稳定事实投影。
- [ ] C-AC7：serve/export/codex-tracer/dash/tui启动与弃用说明都翻译，但Web/desktop资产/API和TUI面板未被本子任务改动。

## 范围外与风险

最危险是把JSON-only默认命令当成人读、所有路径用同一个confirmation默认、翻译缓存串用或日志原文修改。回滚不执行外部更新/删除，保留旧cache和user labels。

不新增第三语言/热切换，不修改Web/desktop语言、token/query/sync算法，不操作真实usage或凭据，不安装/提交/推送/发布。没有阻塞产品决策；技术机制见design，执行前仍需用户明确解除不实现限制。
