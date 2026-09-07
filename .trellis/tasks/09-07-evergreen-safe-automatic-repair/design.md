# 阻止普通同步与启动修复清空历史：设计

## Mechanism and tradeoffs

推荐采用最小行为修正：停止隐式重建，保留历史并给出显式修复指引。这是提交给用户审批的行为选择，尚未获准。保留现有 legacy 检测和 SourceSyncStatus warning；engine 将 legacy parser 从本轮写入集合移除，避免旧 cursor/旧口径继续混写；不是只删 reset 然后照常解析。serve 复用同一检测/警告逻辑，不再调用 destructive rebuild。已有 run_log 可以记录本轮状态，但不伪造来源已修复。成功/current/显式 rebuild 路径沿用原协议。

替代方案是分源暂存所有 shards，全部解析成功后原子替换历史；这会扩展 writer、失败恢复和大数据暂存边界，本轮不选。若用户要求保留自动修复体验，应重新审批该方案，不能执行中自行扩大。

## File ownership

- `src/sync/engine.rs`
- `src/commands/serve.rs`
- `src/commands/source_status.rs`
- `src/store/sync_status.rs`
- `tests/sync/accounting.rs`
- `.trellis/spec/llmusage/backend/token-accounting-contracts.md`
- `README.md`
- `README.zh-CN.md`
- `docs/reference/cli.md`
- `docs/zh/reference/cli.md`

## Tool and model assignment

同步修改source-status与Store状态读取中的旧“run unbounded sync for automatic safe repair”指引，避免用户被引导回已停用路径。状态warning可以更新，历史usage/raw/bucket/behavior/cursor/source_file不可改写。bounded与unbounded普通同步都不得让legacy来源混入新口径；沿用来源级跳过+明确警告，文档不再要求先跑一次无界sync。

Codex 强模型或 Claude Code 强模型负责保留历史的语义与独立审查；在批准行为和红色回归后，可将 warning/copy 与测试补齐交给便宜模型，engine/serve 控制流由强模型把关。

## Failure and rollback

单项提交前用diff保留无关改动；失败只撤销本任务补丁，不重置用户工作树。不存在自动发布、自动升级远程或全局设置的授权。门禁失败需定位具体操作；未执行的原生/远程验证保留UNVERIFIED。

## Documentation writeback

批准并通过验收后同步上述拥有的项目说明/spec；适用工具标注为 Claude Code、Codex、Grok Build、Kimi Code、OMP。跨工具公共说明由 harness-contracts 子任务最终汇总。任务计划本身不是已生效规则。
