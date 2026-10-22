> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# S 技术设计

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：已实施并通过完整 Rust 门禁；独立验收见 `research/check-evidence.md`。** 前置：10-20-cli-language-foundation。Trellis父子关系不自动调度这些依赖。

## 架构与数据流

1. CLI专用console formatter捕获Language，按稳定event_code或pricing的operation/phase格式化状态/阶段/关键字段；完整event经既有StderrLogWriter/sink/renderer路由。RUST_LOG不变，进度独立于file warn filter。
2. 所有CLI可达app-owned tracing登记英中模板，包括debug/trace；可选code追加在原fields，原message/level/target/版本计数保留。unknown external与历史记录有标记的raw fallback。logs human消费者复用此接口，fileNDJSON不经翻译。
3. engine新增crate-private RunNotice及反馈回调；public入口None委托同一内部实现，CLI收集本轮typed事实，见父design第6节的每项来源。禁止从last_error子串判severity或末尾重扫coverage/用旧状态猜choice。
4. CLI recovery adapter负责本地化提示与读写，捕获现有coverage和选择；通过renderer暂停/恢复写完整行，不改变engine/parser安全判断和k/r规则。
5. summary纯渲染按display width计算十列，增加闭合外框、竖线、header/total分隔，数值右对齐不裁切；诊断在bottom之后按source排，保留code/样本上限/隐私及fault-vs-info语义。
6. 错误/取消/早退都复用scoped sink恢复；JSON/emit-shards不安装human stdout内容。NO_COLOR/LLMUSAGE_NO_COLOR与nonTTY不输出ANSI。

## 兼容与接口

父design是共享接口真源；新公开类型/必填字段、schema/wire/accounting变更不在范围。人读文本从语言目录投影，数据/代码/用户值保留；原错误source、machine分支与既有退出码不变。共享修改需遵循上述文件责任，不能回退其他child的工作。

## 关键依据

- 父目录 `research/surface-coverage.md` 是完整覆盖与归属清单。
- 精确源码事实与相关合同在本任务implement/check清单的research/spec中，执行前核对未漂移。
- 所有功能验证当前NOT_RUN；不把规划或context校验当成实现证据。

## 回滚与限制

最危险是把翻译写回持久化状态/协议、回调改变事务、缩窄数值或误改恢复确认。回滚只回展示与私有feedback，不真实重建或降级数据版本。

不做数据/凭据/历史日志语言迁移。native验收独立于headless；不建立尺寸/DPI/断点矩阵。

## 已授权的中立错误事实补充（2026-10-20）

独立审查确认三类安全拒绝和固定 SQLite 打开失败仅靠原文本无法正确双语投影，协调代理批准如下有限补充：

- `sync::failure` 的 crate-private `SyncFailure` 保存原 canonical Display 和中立 `FailureKind`（有损重建风险、OMP 拆分前置检查、无解析器的重建来源）。风险事实保留 host/source/缺失文件数/受保护事件库存；不计算或宣称精确丢失数。只替换原拒绝点的错误载体，原判断、查询、重置顺序及错误文本不变。
- 既有 run-failed tracing 点追加可选 `failure_code/failure_facts`；console/log replay 按类型化事实投影，file 的原 message/error 保持。未知或不完整元数据保留原字段，不能按错误字符串识别类型。
- human reporter 排空后使用私有失败渲染适配器；公开 `SyncEvent::Failed.error` 继续保存 canonical。最终 application_error 同样投影中立类型。`--since` 在既有 CLI producer 处附加 Diagnostic，chrono source 保留。
- `parsers::feedback` 使用 Tokio task-local collector 包围既有 driver future。OpenCode/ZCode 只在原打开失败分支发中立提示；private DriveResult 交 engine notice。公开 trait、DriveContext、SourceSyncStats、SyncEvent 无新增字段或签名变化；未安装 collector 的直接 parser 调用行为不变。
- SourceFileListing 在原 inventory 错误产生点记录私有索引 provenance，原 errors/error_summary 不变。只有六个 built-in `inventory_error` 赋值点调用 `error_summary_with_feedback(source)`；单纯发现/ownership/coverage 查询不产生该来源的失败。样本仍为原 inventory 上限 3（parse-issue 样本仍为 8），保留 total/omitted，排除 ownership skip；额外未知错误明确保留为原始详情。
- WalkDir 的人读 OS 详情来自 `io_error().to_string()`，不携带 WalkDir 包装中的完整路径；无 OS 原因时投影中立 traversal 类型。原 warning/error/stat/file 文本继续保留。既有 inventory warning 点仅追加必要中立字段，人读已知模板不再次展示自有英文包装。

回归以真实临时数据库及子进程验证 en/zh、RUST_LOG off/error、JSON/分片 canonical 字段和退出码，collector 另验证嵌套、异步隔离、drop/错误恢复。源码最终快照与实现者/审查者验证分别记录在 research。
