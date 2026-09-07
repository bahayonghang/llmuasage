# 阻止普通同步与启动修复清空历史：实施顺序

## Preconditions

- 用户明确批准本子任务最新PRD/design。
- 第一优先；无需等待门禁修复。与 remote-accounting 都修改 engine/状态时必须串行。
- 强模型检查git diff与现状；读取implement.jsonl、check.jsonl；只启动当前子任务，不启动父任务代替全部子任务。

## Steps

1. 在隔离fixture中复核本任务事实和验收失败条件，记录修改前基线。
2. 按design修改拥有文件；先最小改动再跑定向检查，文件所有权重叠的工作串行。
3. 对照每个AC逐项记录结果，不以日志中出现expected error判定测试失败；以退出码和断言为准。
4. 通过必要检查后停止重复全量验证。强模型独立复审，再回写本任务拥有的spec/说明并标注适用工具。
5. 向父任务提供改动、命令/退出码、未验证项及回写位置。提交/远程交付按用户当时明确授权执行。

## Validation commands

- `cargo test --locked --all-features --test sync accounting -- --test-threads=1`
- `cargo test --locked --all-features --test sync runtime::reset -- --test-threads=1`
- `cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1`
- `python scripts/ci-rust.py`
- `npm --prefix docs run docs:build`

Windows 命令使用当前PowerShell；文中占位任务路径以此目录的实际路径替换。本轮检查证据见父任务 research/test-results.md；本清单是实施后必跑要求，不是已完成声明。
