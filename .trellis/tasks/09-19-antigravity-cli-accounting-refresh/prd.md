# Antigravity CLI 计数语义校正与现代 SQLite 解析

## Goal / Background
修复起始版本 `d1108b0:src/parsers/antigravity.rs:466` 与现代 ModelUsageStats 合同不一致的计数，并补齐时间/steps/retry。负责父任务 R2、CLI R3/R4。最终字段证据见父 research/native-semantic-validation.md；原始研究见 native-sqlite-findings.md。

## Requirements
- R1 语义：验证 #1 模型编号、#2 input、#4/#5 cache、#9 reasoning/#10 visible 与 total；正常/空/失败/retry 有真实脱敏样本和独立 oracle，不机械保留旧公式。
- R2 完整性：现代 typed 时间/steps/retry、模型 effort、稳定多身份去重，未知形态诊断。不能用 mtime/任意 cache bytes 推断时间。
- R3 重放：WAL 变化、一致只读快照、跨库镜像、失败/取消/bounded/history 保护。
- R4 版本/历史：新旧语义隔离、显式修复路径和 protected hook history 行为明确；不得无条件升版本后使用户无法恢复同步。

## Acceptance Criteria
- [x] AC1（R1）：真实六通道 oracle 覆盖正常/空/失败、模型数字编号和 retry；正 cache write 与 thinking-only/visible-only 使用明确标注的 descriptor-based 构造回归，不能声称为原生正样本；旧样本独立重算审查后建立回归。
- [x] AC2（R2）：跨日/idx 缺口/steps-only/retry、重复 response/provider-message/message identity、冲突关联和未知模型行为明确；同生成只计一次、独立 retry 不误合并。
- [x] AC3（R3）：二次 sync、DB/WAL-only 更改、一致读取、缺失/损坏/忙 DB、取消/bounded/full 保留与 atomic group replay 验证通过。
- [x] AC4（R4）：当前 marker2 的可恢复 parser 行和不可恢复 hook 行分别处理；普通 sync 不混语义/自动重建；显式修复既有可执行路径又保护历史；remote marker/host 回归通过。

## Scope / readiness
实施与验收完成，证据见父 research/implementation-validation.md。E1 独立 descriptor、原生脱敏样本和整数 oracle；正 cache-write/单输出形态为明确构造证据。E2 staged parser-only replacement 保留 hook 行并披露旧语义；v21固定2、现代marker3。旧 remote 历史保持拒绝，不新增RPC、框架或自动回填；任务保留in_progress，未commit/archive。
