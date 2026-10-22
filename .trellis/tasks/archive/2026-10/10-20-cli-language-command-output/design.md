> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# C 技术设计

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation, 10-20-cli-language-sync-console。Trellis父子关系不自动调度这些依赖。

## 架构与数据流

1. 每个C清单项目分别定位success/empty/rejected/failure及machine分支；人读renderer在F的locale scope下使用commands目录，数据DTO保持原值，禁止序列化后替换英文。
2. CLI report_table仅做标题/表头/注释/empty/总计文案及中文display width，保留token visible-channel sum、JSON authoritative totals、no-cost/compact/source颜色/单位。--locale仍原值透传，不控制lang。
3. typed domain/error code和private CLI diagnostic context提供自有错误本地化，保持原cause。source-status/doctor/cleanup不要靠英文detail子串分类；已知producer需给中立字段，unknown历史/外部detail有标记保留。
4. statusline翻译自有today/block/projected/cost标签，保持一行与hook输入/数据单位。读写latest.en.txt/latest.zh.txt，旧latest.txt不跨语言回退且不主动删。
5. logs表头/统计/运行记录状态按语言显示；event消息用S的stable-id projector，JSON仍原记录。diagnostics默认JSON和--out文件不翻译，仅写出/forget-file确认及错误翻译。
6. update保持默认Y/y/yes/n/no、EOF失败/二次ref核验，Cargo/git继承的外部输出不译；remote删除仍要--yes，clean/uninstall仅原owned范围。serve/export/codex-tracer仅CLI说明，Web内容不变。

## 兼容与接口

父design是共享接口真源；新公开类型/必填字段、schema/wire/accounting变更不在范围。人读文本从语言目录投影，数据/代码/用户值保留；原错误source、machine分支与既有退出码不变。共享修改需遵循上述文件责任，不能回退其他child的工作。

## 关键依据

- 父目录 `research/surface-coverage.md` 是完整覆盖与归属清单。
- 精确源码事实与相关合同在本任务implement/check清单的research/spec中，执行前核对未漂移。
- 所有功能验证当前NOT_RUN；不把规划或context校验当成实现证据。

## 回滚与限制

最危险是把JSON-only默认命令当成人读、所有路径用同一个confirmation默认、翻译缓存串用或日志原文修改。回滚不执行外部更新/删除，保留旧cache和user labels。

不做数据/凭据/历史日志语言迁移。native验收独立于headless；不建立尺寸/DPI/断点矩阵。


## C 实施边界补充：命令错误日志与旧集成清理

现有 run_tracked/run_untracked 会在最终错误展示之前记录 canonical error。仅有 Diagnostic 的双语字符串不足以支持日志重放：允许 CLI Diagnostic 携带可选私有封闭 code/enum 与中立参数，纯语言目录同时投影最终 UI 和日志。既有 .03/.04 发射点只新增可选 diagnostic_code/diagnostic_facts，保留原 message/error/level/run 记录；只有完整校验并成功投影才隐藏人读原字段，未知/畸形/不匹配仍明确保留原始详情和元数据。禁止写入英中模板、按错误文本反推 code，或抑制关键错误说明。报表和维护实施者协作使用此接口。

IntegrationAction 的既有 source/status/detail 与公开 cleanup/cleanup_all 签名保留。允许四个旧集成清理 producer 和 integrations/mod.rs 在原决定点通过私有同调用结果/反馈传递 residue/unowned/noartifact/notify/plugin 等事实；公共入口委托同一逻辑，uninstall 人读消费者投影事实。聚合失败若含自有上下文，使用私有中立失败事实保留 exact canonical Display；外部 OS/serde 详情保持原文。不重扫、不解析 detail、不增加全局状态，不改变恢复、备份、删除或审计顺序/集合。用临时配置比较动作、审计和文件字节等价。

发射点补充：同一中立元数据规则也覆盖既有 update .05。无 Diagnostic 的已知 LlmusageError（例如未初始化）需按其现有 typed variant 投影日志说明，避免最终错误已翻译、console 仍漏原文；保留 F 的原错误链和20项语言回归。外部未知错误仍明确原文，不反推字符串。

元数据完整性：新诊断与已有私有 sync failure 的日志解码，仅在包含嵌套参数的完整已知形状校验成功后隐藏原字段；serde 默认忽略额外字段不能成为丢弃未来详情的理由。允许最小严格解码/属性和英中 fallback 回归，不改变 producer 序列化或安全决策。

复用补充：既有 sync_summary::parse_issue_lines / notice_lines 仅提升到 pub(crate)，供 status/source-status/doctor 共用已验证的隐私、样本上限与 fault/info 投影。可用只携带既有 ParseIssues 的临时 stats 适配纯 helper 输入；不将默认/虚构计数写入输出、DTO 或存储，不改变 helper 行为。

2026-10-21 续接补充：source-status 人读输出不能用 display_name 代替原 detail，导致丢失 reader 限制和实际探测标签。允许 domain/platform_monitor.rs 在现有 probe 构造处通过 crate-private 同调用 helper 返回语言中立标签，公开 PlatformProbe 与函数保持原样委托。未检测到根时标签仍来自 descriptor.roots；已检测到根时仍来自实际 detected 集合。不重新扫描、不解析 canonical detail、不在 domain 读取语言，原 detail/JSON/路径隐私保持逐字相同，并补等价回归。

校验错误补充：独立检查在 catalog apply 的无效价表及 remote add 的保留标签拒绝路径复现了自有英文详情漏译。ConfigInvalid 仅保存 String，不能在 renderer 反推分类。允许 pricing-only 内部 parse/validate/load/store 核心传播私有 PricingFailure { canonical, facts }，公开及既有共享 API 作为 wrapper 返回完全相同的 LlmusageError；CLI 私有入口取中立 PricingFacts 后接现有 Diagnostic/log 投影。允许 remote/register.rs 同样在原校验决定处返回可选中立 facts 的私有结果，公开接口保留。不得使用 TLS/global 错误缓冲、重复读/parse/校验/网络，或更改价表算法、锁、事务、文件顺序、远程副作用与 validation 顺序。配对进程测试须同时核对 canonical Display/variant、持久化记录和原成功行为。

后续确切夹具证明 meta.pricing_recompute_in_progress 为损坏 JSON 时，catalog reset 在 bootstrap 阶段先失败，仍丢失自有 marker 分类。允许 store/schema.rs 的原私有 bootstrap 核心使用上述 PricingResult，并提供 crate-private bootstrap_with_pricing_facts 给 catalog/doctor refresh；公开 bootstrap wrappers 解回原 LlmusageError。锁、迁移、upgrade 调用、读取次数及执行顺序均保持。只修复此已复现传递缺口，不推广成通用 bootstrap 错误框架。

一致性补充：上述已复现 bootstrap 价表错误在 C 的 init/uninstall/remote/diagnostics forget/export/serve 原生产入口复用同一私有入口及 cli_errors::pricing_result。测试夹具 setup 不改，S 的同步回调单独交整合审查。包装错误的进程回归还证明 run_log 使用 `{err:#}` 会重复新增展示 context 的 canonical 文本；允许 Diagnostic 使用明确的私有展示包装标记，canonical_detail 仅跳过该标记包装并保留其余完整原链，run_tracked 仅切换此持久化格式投影。既有构造器行为不变，不按字符串去重，不移除 typed cause，不把 UI cause 展示与持久化格式混为一谈。

远程导入补充：source 复核确认 commands/remote 的 RemoteError 直接回显 importer/decoder 的自有校验英文，warnings 也直接回显唯一 skipped-lines 文案。允许 importer.rs/protocol.rs 在原决定处通过私有 ImportFailure/中立 facts 与 warning facts 同调用返回，公开 import/next_record/ImportOutcome/ShardRecord/errors 均保持 canonical 原值及签名。CLI record_contact 和 run 数据先保存原错误，再选择人读投影；已知失败的立即 stderr、最终错误及日志共享 facts。禁止重读/解析自然语言、重扫、改变提交/Pi-OMP/accounting/watermark/finish/锁/协议规则；只能用既有 Memory/Scripted seams 复现。新 worker 独占此分支与 fake 回归，维护 owner 仅协助共享 facts 注册。
