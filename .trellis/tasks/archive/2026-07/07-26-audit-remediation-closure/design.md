# Design: 审计整改二次闭环

## Architecture

父任务只提供需求索引、执行顺序和集成 gate。每个子任务拥有一个独立行为边界、focused tests 和可回滚提交，避免再用一个宽泛提交掩盖未完成验收项。

## Execution Order

1. `msrv-validation-baseline`：先恢复可信基线，修正格式与测试执行证据。
2. `write-fencing-closure`：先封闭所有 mutation 的并发正确性边界。
3. `integration-atomic-replace`、`sync-job-contract-closure`、`bounded-jsonl-reader`：修复数据/契约正确性。
4. `immutable-self-update`、`runtime-log-bounds`、`public-read-security-boundary`：修复供应链、运行期与暴露面。
5. `arch-dependency-enforcement`：在行为稳定后收紧依赖结构和 CI gate。
6. 父任务执行独立集成复审，不以子任务状态代替源代码与测试检查。

## Cross-Task Contracts

- 所有 mutation 路径最终都必须经过同一个 fenced write permit；子任务不得各自实现不兼容的锁判断。
- API/CLI 共享 typed validation；Web adapter 只映射错误，不重新解释 `None`。
- 日志、diagnostics 和 public routes 的安全策略必须一致，避免一个入口脱敏、另一个仍泄露。
- CI/MSRV 子任务提供最终命令来源；其他子任务只增加 focused tests，不复制另一套 gate。

## Compatibility

- loopback CLI/Web/TUI 的合法输入与输出保持兼容。
- stable 更新从 branch 语义收紧到 release/tag 语义属于安全修正；dev channel 必须继续明确提示可变/不稳定风险。
- `recent_days` 按原产品含义实现真实裁剪，不通过改名弱化既有调用方预期。

## Rollback

- 每个子任务独立提交，失败时只回退对应行为面。
- 数据库 schema 若需新增 fencing 状态，必须提供向前迁移测试；不执行破坏性降级。
- 外部配置写入使用备份/事务式记录恢复，测试不得修改真实用户配置。
