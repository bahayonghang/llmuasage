# 校验远程来源计费口径：设计

## Mechanism and tradeoffs

SourceSyncStatus的version当前只是运行时字段，src/store/sync_status.rs并不把它持久化。最小方案复用现有meta表，以host/source命名空间保存已认证历史版本；不新增数据库schema或把本地全局marker当远程真值。source-status及状态加载对remote读取这个host/source证据。Header添加本次来源->expected accounting version映射并提升SHARD_PROTOCOL_VERSION，emitter根据registry列出真实来源。Importer在读取Header后、任何shard commit前完成整个来源集合验证；decoder负责结构/wire版本，不重复做语义验证。header未列出的shard来源拒绝，不根据llmusage版本字符串推测口径。

保持schema_version诊断含义，不强制schema相等。一次当前版本的增量stream不能证明历史全部当前：若remote来源已有行但历史marker未知/不兼容，则禁止该来源增量混写，明确提示需要获准的完整恢复，保留旧行与watermark；不自动回填/清空。只有来源本来为空、请求无since且完整trailer成功且该来源无parse error时，才建立历史marker；已有匹配marker可正常增量。中途失败保留现有分shard提交行为但不推进marker/watermark，后续状态诚实显示unknown。完整历史恢复流程不在本任务中实现，不宣称已修复旧数据。

## File ownership

- `src/remote/protocol.rs`
- `src/remote/importer.rs`
- `src/commands/sync.rs`
- `src/commands/source_status.rs`
- `src/store/sync_status.rs`
- `src/store/schema.rs`
- `tests/remote/shard_transport.rs`
- `tests/remote/lifecycle.rs`
- `tests/sync/accounting.rs`
- `docs/adr/0014-ssh-remote-host-import.md`
- `.trellis/spec/llmusage/backend/source-sync-contracts.md`

## Tool and model assignment

Codex/Claude Code强模型规划协议与host/source真值边界，Grok Build可独立审查Grok来源合同；便宜模型只负责确定的字段接线/fixture。不能让便宜模型自行决定兼容或历史回填。

## Failure and rollback

单项提交前用diff保留无关改动；失败只撤销本任务补丁，不重置用户工作树。不存在自动发布、自动升级远程或全局设置的授权。门禁失败需定位具体操作；未执行的原生/远程验证保留UNVERIFIED。

## Documentation writeback

批准并通过验收后同步上述拥有的项目说明/spec；适用工具标注为 Claude Code、Codex、Grok Build、Kimi Code、OMP。跨工具公共说明由 harness-contracts 子任务最终汇总。任务计划本身不是已生效规则。
