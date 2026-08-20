# C3 执行清单

设计依据：父 `design.md` §6；本任务 `design.md`。

前置：C1 完成并通过 G1。不依赖 C2。

## 顺序清单

- [ ] 1. `src/query/filter.rs`：`QueryFilter` 增加 `host_id`，在 `sql_filter_with_model_column` 内追加 host 条件。
- [ ] 2. `src/query/reports.rs`：`ReportFilter` 增加 `host_id`；`push_bucket_filter` 与 `visit_filtered_events` 追加 host 条件；新增 `load_daily_reports_by_host`、`load_monthly_reports_by_host`、`load_weekly_reports_by_host`；按本任务 design.md 的判断标准决定抽取公共分组实现还是平行新增。
- [ ] 3. `src/commands/report_args.rs`：`--host` 参数与 `to_filter` 的 label 解析；未注册 label 报错并列出候选。
- [ ] 4. `src/commands/focused.rs`：focused 命令（claude / codex / opencode / antigravity）接入 `--host`。
- [ ] 5. `src/commands/unified_report.rs`：每主机行进入 CLI 表格与 JSON 报表。
- [ ] 6. `src/commands/source_status.rs`、`src/commands/diagnostics.rs`：输出按 host 区分。
- [ ] 7. `src/web/mod.rs`：`/api/dashboard` 增加 `hosts` 字段；确认不进入 `--public` allowlist。
- [ ] 8. 用 Bash heredoc 新建 `src/web/assets/render/hosts.js`（镜像 `render/sources.js`），并用 Bash 修改 `app.js` 注册渲染、`copy.js` 增加中英文案。不要用 Edit / Write 工具改这些 JS 文件。
- [ ] 9. `hosts.length <= 1` 时隐藏 dashboard 主机分组。
- [ ] 10. 测试：报表 host 过滤与合计一致性、未注册 label 报错、behavior/Activity 在 host 过滤下一致、dashboard payload 合计与 CLI 一致。
- [ ] 11. `cargo fmt`，然后跑验证命令。

## 验证命令

```bash
cargo test --test report_commands -- --test-threads=1
cargo test --test web_sessions_endpoint -- --test-threads=1
cargo test --test hour_of_week -- --test-threads=1
cargo test --all-features -- --test-threads=1
cargo clippy --all-targets --all-features -- -D warnings
node --check src/web/assets/render/hosts.js
node --test scripts/tests/dashboard-render-lifecycle.test.mjs
```

## 风险与回滚

| 风险                                                         | 位置                | 处置                                                                          |
| ------------------------------------------------------------ | ------------------- | ----------------------------------------------------------------------------- |
| 全局 prettier hook 把 JS 单引号改为双引号，CI 的 JS 检查失败 | `src/web/assets/**` | 这些文件只用 Bash heredoc 写入                                                |
| 报表分组重构牵动既有 per-source 测试                         | `query/reports.rs`  | 既有测试需改动即改为平行新增，不重构                                          |
| dashboard payload 超出性能预算                               | `web/mod.rs`        | 主机聚合复用 sources 的查询形状；按 `dashboard-performance-contracts.md` 复核 |
| 主机 label 经 `--public` 泄露主机名                          | `web/mod.rs`        | `hosts` 字段不进 public allowlist                                             |
| 单主机用户看到空面板                                         | `render/hosts.js`   | `hosts.length <= 1` 隐藏                                                      |

本子任务无不可逆改动，回滚为撤销代码改动。

## 注意
`dashboard-performance-contracts.md` 为 40299 字节，超过 context_injection 的 32768 上限，注入时会被截断。实现前直接打开该文件读 payload 与查询预算章节，不要只依赖注入内容。
