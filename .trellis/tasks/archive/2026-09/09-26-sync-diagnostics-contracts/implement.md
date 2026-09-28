# 诊断任务实施计划

## 已批准的门禁修复

2026-09-27 继续执行用户的范围扩展指示：先修复既有门禁，再继续后续两个子任务。追加检查：

- [x] G1：查明并修复 `local_timezone_date_bounds_use_current_fixed_offset_snapshot`、`api_trends_daily_exposes_daily_cost_series`、`behavior_apis_return_activity_tools_and_snapshot_fields`；保留明确的本地时区、IANA 时区与 UTC 边界语义。相关 30 项定向测试通过，独立代码审查通过，完整门禁单独记录。
- [x] G2：依据 `semver-check.log` 的 11 类正式差异建立处置清单，保持新来源区分、remote host 隔离和 source-reported cost；选择并记录兼容性/版本边界，再修复正式门禁。
- [x] G3：完整 CLI suite 34/34 与 `just ci` 通过，证据见 `ci-native.log`、`ci-native-result.json`。先前偶发 `status_failure_emits_error_without_run_log` 失败原因仍未查明；本次完整 suite 未复现。

独立检查继续处理 Antigravity 行级 decode 故障与来源故障分类。`semver-head-comparison.log` 仅是当前诊断改动的增量证据。所有旧失败日志继续保留。

G2 研究见 `research/semver-remediation.md`。用户于 2026-09-28 选择推荐的 2.0.0 开发树版本边界并补迁移说明。按 13 个实际版本入口同步 metadata，通过包管理器更新本地包锁项；第三方依赖保持。随后执行正式 SemVer、独立检查和完整本地门禁，再继续两个优化子任务。正式 baseline 与 lint 策略保持不变。

## 原定实施步骤

- [x] 读取父任务研究和 source-sync/token-accounting/runtime-log 契约，核对 HEAD。
- [x] 在真实 domain/formatter/store seam 写最小失败回归：缺成员被误计、reason 遮蔽 offset、Grok 定位不匹配、13/8 省略提示。
- [x] 定义私有持久化 DTO/crate-private 结果及兼容访问器，保留现有公开类型；完成全部状态读写入口 round-trip 与所有“无错误”安全守卫检查。
- [x] 修正 parser 的稳定错误码与 Grok 文件关联；不修改原生 tokens。
- [x] 修正摘要和 source-status 展示；用 renderer/日志组合验证并修复已复现的混行。
- [x] 更新现有契约与 README/双语 CLI 文档，交给独立 check。

验证命令：

~~~text
cargo test --locked --all-features --lib parse_issues -- --test-threads=1
cargo test --locked --all-features --lib commands::sync_summary -- --test-threads=1
cargo test --locked --all-features --lib commands::sync_progress -- --test-threads=1
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test cli -- --test-threads=1
cargo test --locked --all-features --test remote -- --test-threads=1
cargo test --locked --all-features --test query -- --test-threads=1
cargo semver-checks --baseline-rev v1.2.0
just ci
~~~

实现新增 case 后确认过滤器实际匹配测试，0 tests 不算通过。先定向执行，最终跨表面门禁只需成功一次。普通用户数据库不参与测试。出现 token/cost/marker 变化则回滚候选并定位问题。所有 D1–D5 完成前保持任务未完成。

2026-09-28 验收：13 文件/15 版本字段一致；双语迁移条目由独立检查核对实际源码。正式 SemVer exit 0；ci-v2-result.json 记录完整 just ci exit 0、276.111 秒、锁文件不变。D1–D5/G1–G3 完成，允许继续依赖子任务；提交与归档仍待统一提交确认。

## 提交与归档交接（2026-09-28）

用户已确认交付方案。工作提交 `cc7eed3`、`6389ceb` 已完成，产品文件字节保持不变。验收及门禁沿用已通过证据；本轮不重复测试。任务随后使用 Trellis 归档脚本标记 completed 并归档，context 路径已同步到同月归档位置。
