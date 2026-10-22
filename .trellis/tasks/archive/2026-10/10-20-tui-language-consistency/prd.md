> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# T：让dash/tui九面板与所有公共控件、弹窗、异步状态和同步覆盖层读取同一语言偏好，中文下交互与数值语义保持正确。

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation。Trellis父子关系不自动调度这些依赖。

## 目标与范围

让dash/tui九面板与所有公共控件、弹窗、异步状态和同步覆盖层读取同一语言偏好，中文下交互与数值语义保持正确。

R10-R11、R7-R8；父AC1/AC8/AC13-AC14；surface清单T01-T10。

## 文件责任

- `src/tui/app.rs`
- `src/tui/mod.rs`
- `src/tui/draw.rs`
- `src/tui/nav_bar.rs`
- `src/tui/footer.rs`
- `src/tui/help_dialog.rs`
- `src/tui/source_picker.rs`
- `src/tui/sync_control.rs`
- `src/tui/quota.rs`
- `src/tui/data_loader.rs`
- `src/tui/panels/`
- `src/common/i18n/tui.rs`
- `src/subscription/types.rs`
- `src/subscription/mod.rs`
- `src/subscription/cache.rs`
- `src/subscription/claude.rs`
- `src/subscription/codex.rs`
- `src/subscription/grok.rs`
- `src/subscription/kimi.rs`
- `src/subscription/http.rs`
- `tests/tui/`

report_table.rs归C；subscription改动仅增加crate-private中立展示hints及私有cache可选metadata，publicDTO/凭据/请求策略不变。Web/desktop/CLI startup与JobRegistry不归本任务。

## 验收标准

- [ ] T-AC1：同root lang zh/en后重新启动dash与tui分别中文/英文；无新TUI语言菜单，无后台设置轮询，既有热键不变。
- [ ] T-AC2：TestBackend对T01-T10逐类覆盖data/loading/empty/error/dialog/period-detail，包含九面板与footer/narrow已有分支；新自有文案不落raw fallback。
- [ ] T-AC3：中文导航各标签绘制/点击命中同一Panel，sort箭头/键、j/k/Enter/Esc/鼠标选择与en的逻辑状态一致。
- [ ] T-AC4：quota fresh/live都显示正确翻译的自有诊断和绝对额度说明，原report数字完全一致；new/oldcache/hints缺失/错指纹、TTL/cache_hit/请求次数覆盖，publicDTO/reader兼容。
- [ ] T-AC5：凭据bytes前后一致、邮箱默认隐藏与m、r旁路cache/R不轮询、取消/generation拒旧结果保持；所有端点为本地mock。
- [ ] T-AC6：x/y同步过程/完成/失败/取消本地化，JobRegistry/worker lock与退出restore保持，不依赖commands::sync或自动接受有损重建。
- [ ] T-AC7：idle tick/dirty redraw、visible row window、theme/noColor及number formatting既有回归通过；native普通窗口证据交A补充，headless不冒充native。

## 范围外与风险

最危险是只翻译tab漏掉dialogs/worker字符串、中文点错导航、变更quota公开payload或从百分比造绝对余额。可选cache元数据必须兼容缺失/旧reader，不改凭据或触公网。

不新增第三语言/热切换，不修改Web/desktop语言、token/query/sync算法，不操作真实usage或凭据，不安装/提交/推送/发布。没有阻塞产品决策；技术机制见design，执行前仍需用户明确解除不实现限制。
