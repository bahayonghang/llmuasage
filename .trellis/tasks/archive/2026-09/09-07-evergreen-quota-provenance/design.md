# 统一配额缓存命中来源：设计

## Mechanism and tradeoffs

让subscription fetch_all返回明确的UsageFetchOutcome { report, cache_hit }，在实际cache::load成功分支置true，fetch_live分支置false；更新所有仓库内调用者（当前为desktop runtime、src/tui/quota.rs及subscription单测）。新增类型放在现有subscription/mod.rs，不增加兼容包装层。删掉桌面cache_file_fresh和重复TTL判定，desktop现有QuotaResponse保持相同前端形状。API变化与semver-workflow结果联动：这是本P2提案明确包含的公开Rust返回类型变化，必须由强模型核对semver结果；需要版本发布调整时另行审批，不静默发布。测试使用本地endpoint与tempfile，mtime通过std文件时间接口设置，不读真实账户。

## File ownership

- `src/subscription/mod.rs`
- `src/subscription/cache.rs`
- `desktop/src-tauri/src/commands/runtime.rs`
- `desktop/src-tauri/tests/quota.rs`
- `src/tui/quota.rs`
- `.trellis/spec/llmusage/backend/tui-subscription-contracts.md`

## Tool and model assignment

强模型确认返回类型与公开API；便宜模型适合mtime移除、调用者机械改动和四种fixture，强模型复审网络/缓存真实性。

## Failure and rollback

单项提交前用diff保留无关改动；失败只撤销本任务补丁，不重置用户工作树。不存在自动发布、自动升级远程或全局设置的授权。门禁失败需定位具体操作；未执行的原生/远程验证保留UNVERIFIED。

## Documentation writeback

批准并通过验收后同步上述拥有的项目说明/spec；适用工具标注为 Claude Code、Codex、Grok Build、Kimi Code、OMP。跨工具公共说明由 harness-contracts 子任务最终汇总。任务计划本身不是已生效规则。
