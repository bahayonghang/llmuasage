# 统一配额缓存命中来源：实施顺序

## Preconditions

- 用户明确批准本子任务最新PRD/design。
- test-gates后实施；公开API变化必须在semver基线修复后重新审查。
- 强模型检查git diff与现状；读取implement.jsonl、check.jsonl；只启动当前子任务，不启动父任务代替全部子任务。

## Steps

1. 在隔离fixture中复核本任务事实和验收失败条件，记录修改前基线。
2. 按design修改拥有文件；先最小改动再跑定向检查，文件所有权重叠的工作串行。
3. 对照每个AC逐项记录结果，不以日志中出现expected error判定测试失败；以退出码和断言为准。
4. 通过必要检查后停止重复全量验证。强模型独立复审，再回写本任务拥有的spec/说明并标注适用工具。
5. 向父任务提供改动、命令/退出码、未验证项及回写位置。提交/远程交付按用户当时明确授权执行。

## Validation commands

- `cargo test --locked --all-features --lib subscription -- --test-threads=1`
- `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --test quota -- --test-threads=1`
- `npm --prefix desktop test`
- `python scripts/ci-rust.py`
- `cargo semver-checks --baseline-rev v1.2.0`（设计涉及公开签名变化，必须记录并审查 API 差异。）

Windows 命令使用当前PowerShell；文中占位任务路径以此目录的实际路径替换。本轮检查证据见父任务 research/test-results.md；本清单是实施后必跑要求，不是已完成声明。
