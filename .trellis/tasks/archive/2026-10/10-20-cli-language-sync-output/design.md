> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# CLI / TUI 双语与同步输出设计

状态：规划完成稿，待用户审阅；用户明确要求先不实现。本文只冻结目标行为与技术边界，不表示代码/测试已经存在。父任务负责整合，不作为产品实现入口。

## 1. 已确认范围与决策

| 决策 | 内容 |
| --- | --- |
| 语言范围 | 全部 CLI 人读帮助、参数错误、运行日志、交互、命令结果，以及 dash/tui 全界面。Web/desktop/browser dashboard 不接管。 |
| 默认与代码 | 无偏好为 English；支持 en、zh，zh-CN 规范化为 zh。系统 LANG/LC_* 不自动覆盖用户选择。 |
| 偏好所有权 | 当前 runtime root 下 `cli-settings.json` 的 `language`；遵循显式 --home > LLMUSAGE_HOME > ~/.llmusage。clean 的已存特例为显式 --home > ~/.llmusage。 |
| 选择入口 | `lang` 在 stdin/stdout/stderr 都为 TTY 时显示 English / 中文（简体）数字选单；缺任一 TTY 则只显示当前值及设置方法，不读 stdin。`lang en|zh|zh-CN` 显式保存。 |
| 取消与失败 | 选单空输入/取消/EOF不写，非法选择显示重试且可退出；直接设置未知语言在写入前拒绝。损坏/不可读偏好回退英语并给一次 stderr 修复提示，读取不覆盖文件；显式保存失败非零、旧文件不截断。 |
| 生效时机 | 每次 CLI 调用读取一次；TUI 从相同 root 取得一次启动快照，退出重开生效。无 TUI 新选单、文件轮询或跨进程热切换。 |
| 兼容入口 | `help --zh` 仍单次强制中文帮助，不保存、不改变其他命令；普通 help/-h 跟随设置。 |
| 报表 locale | 保留 --locale 原解析/传递与 en-US/zh-CN/ja-JP；不让它控制界面语言，不新增数字文化格式或日语 UI。 |
| statusline/导出 | statusline 自有标签跟随语言且维持单行；HTML bundle/JSON/HTTP/SSH 内容语言与 schema 不变。 |

本轮不修改上述 root 优先级的例外、不新增 --lang/env override、不做翻译包下载或通用插件框架。

## 2. 模块与公开边界

候选新增路径是冻结的责任边界，具体小文件划分可在实现时保持等价且更简洁：

| 模块 | 所有权 | 契约 |
| --- | --- | --- |
| `src/common/i18n/` | F 建 Language/目录接口；C/S/T 各维护 commands/logging/tui 目录 | 静态英中成对条目，稳定消息 id 和显式参数；翻译失败不联网。locale-free 的消息标识与 UI 选择分开。 |
| `src/runtime/preferences.rs` | F | 给定 AppPaths/root 的读写；不发现第二份 root，不打开 DB、不获取 worker lock。读返回语言与可选诊断，写仅显式调用。 |
| `src/commands/lang.rs`、`entrypoint.rs`、`cli_errors.rs` | F | 启动、设置、help/Clap 错误和最终 error/exit 的单一人读边界。 |
| `src/runtime/logging.rs` 的 CLI formatter 与 `common/i18n/logging` | S | locale 是初始化时捕获的不可变值；已知事件按稳定标识格式化，file layer 保留。 |
| `src/commands/sync_{progress,summary,recovery}.rs` | S | 使用语言目录；recovery 为 CLI I/O adapter，重建逻辑仍在 engine/parser。 |
| `src/sync/notices.rs` 与 crate-private feedback 入口 | S | 仅中立语义/计数的 RunNotice；不含 locale、翻译文本或终端 I/O，不改变公开 SyncEvent。 |
| `src/tui/` | T；`report_table.rs` 由 C 独占 | TUI 启动/同线程渲染使用固定 locale；工作线程返回原 DTO/事件，不携带可变全局语言。 |

`AppContext`、`AppPaths`、`SyncSummary`、`SourceSyncStats`、`LlmusageError` 等公开类型不增加必填字段；现有 public 调用签名保留。新内部 helper 不迁移旧数据。没有配置作用域时，直接调用纯 public renderer 的基线仍为英语。

## 3. 启动流程

```text
args_os + Clap schema
  -> 只读启动选择（home / command path / help target）
  -> AppPaths 的正确 root 规则（不落盘）
  -> preferences::read（缺失 en；失败 en + 暂存提示）
  -> 正式 Clap try-parse（唯一参数有效性权威）
       help/version/parse-error -> 本地化输出 + 原通道/退出码 -> 返回
       lang                    -> 独立配置操作 -> 返回
       clean                   -> stderr-only logging + 既有 no-create root
       其他                    -> AppContext + CLI console formatter + file logging
  -> locale scope 下 dispatch
       dash/tui -> 同 root locale 快照 + raw mode + event loop
```

只读选择器按同一 Clap metadata 的 flags/short clusters/aliases/arity 跳过值，尊重 `--` 与 OsString。它不运行 validators、不接受命令、不替代 Clap。当前具名参数0/1值形态要有 schema 断言；不能以 hardcoded 选项清单或仅找 argv 中的 `clean`/`--home` 实现。有效的跨命令深度 global home 重复遵循当前 Clap 的最深层覆盖规则（例如 `--home A daily --home B` 解析为 B）；同层重复等非法输入或缺值 home 用首个完整无歧义值或默认 root 决定诊断语言，正式 parse 仍拒绝且无写入。细节见 `research/cli-startup-errors.md`。

帮助基于同一命令树/ArgId 投影：保留根表格组织、按路径生成子命令参数说明，所有框架章节/默认值/可选值文字来自目录。参数拼写和技术取值不翻译；不对整段 Clap 英文 help 做 string replace。

Clap errors 使用 kind/context、Argument ID 约束和安全参数值构造本地化错误。处理所有当前 ErrorKind，未来未知 kind 使用本地化通用段 + 稳定 kind/安全 context，不丢关键信息。直接用官方 error 的通道/退出码，保持 help/version 0、参数错误2；命令执行错误保持1。既有命令特有成功/取消语义不改变。

main 的新 process adapter 在所有 guards 返回后输出最终人读错误并返回 ExitCode，避免 Rust anyhow Termination 再打印一份。保留 `run()->anyhow::Result<()>` 等嵌入兼容入口；lower layer 的原 Error Display/source chain 继续中立。未知 OS/SQLite/git/Cargo 细节可原样附在本地化上下文后。

## 4. 设置存储与 locale 传播

文件最小形态：`{"language":"en"}`。不需要 DB schema 或配置版本迁移。显式语言设置以原 JSON object 为底保留未知字段，仅更新 language；读取损坏文档不改写，明确设置可以完整替换损坏偏好。

写入使用现有 `integrations::write_file_atomic` 的不带 audit 接口（`src/integrations/atomic.rs:478-490`），继承 Windows ReplaceFileW/恢复语义，不复制大型 writer。只有写入时创建 root，使用独立 `cli-settings.lock` 的 fs4 OS 锁串行化 settings writer 的 sibling recovery 操作；不与 worker.lock 混用。文件原子替换使无锁读获得旧或新完整偏好。锁文件保留为已知配置；clean 的 keep 分类包含 settings、settings lock 及该 writer 精确 sibling residue，不扩大删除范围。不要把配置写入记录成 legacy integration 动作。

读取操作没有 settings 锁、DB、目录创建或网络；保存成功后才报告目标语言。初始化路径/文件权限等导致保存失败时，给目标/当前可确定语言的错误与路径，旧偏好保持可用。

locale 传播使用按调用作用域隔离的上下文：CLI async dispatch 由 Tokio task-local scope 持有 Language；同步 TUI/纯 renderer 用可嵌套 RAII thread-local scope，离开/异常后恢复。不要使用进程级可变当前语言，也不向 AppState 添加必填字段。同步 helper 未在 scope 内调用时返回英语。

spawn/spawn_blocking 不继承 locale 是显式约束：human renderer、logger、recovery callback 捕获 Language；异步 parser/query/job/quota worker 不做 UI 翻译。TUI 在主事件/渲染线程把 DTO/SyncEvent 转成当前语言。scope 测试需交错运行 en/zh 两个任务、嵌套作用域及 unwind，证明无串语言。

## 5. 错误、日志和持久化 detail

### 应用错误

- 对公开 typed 错误按 variant/code/字段生成说明；对 CLI 自有 anyhow 错误在生产位置附 typed diagnostic context，保留原 source 和 canonical detail。自有 validator 以 ArgId/明确约束标识生成模板，不解析它先前的英文 String。
- 不通过改变共享 `LlmusageError::Display` 或 query DTO 文案实现 CLI 翻译。
- 对 app-owned 的 status/quality/cleanup/quota 说明，用已有稳定 enum/code 或补 crate-private 中立诊断上下文，不把 free-text substring 当作状态协议；必须覆盖 producer，不能统统归成 unknown。
- 不认识的历史/第三方 free text 原样附在已翻译的「原始详情」中；这是兼容 fallback，不是给新应用文案漏译的豁免。

### 日志

- CLI console layer 使用独立 formatter；基于 `event_code`，或既有 pricing 的 `operation + phase`，输出 `[状态] 阶段 说明 · 关键指标`。普通行不铺满 wall-clock/debug target/长哈希；DEBUG/TRACE 的详细 fields 与文件日志保留完整值。未知第三方日志保留原文和级别。
- 所有可能从 CLI 运行链路发出的 app-owned tracing 消息（含 info/debug/trace）必须登记英中条目与稳定事件标识。标识在原 event 的 fields 内可选追加；不改原 message、level、target、时间/计数或序列化顶层字段，不影响 Web/desktop 原默认 logger。
- RUST_LOG 控制 console，LLMUSAGE_LOG 控制文件；不为了展示进度提高默认过滤级别。bootstrap/source 进度仍从 SyncEvent 来。
- 慢 pricing WARN 保持 WARN，只把信息解释清楚；活跃 progress 与 slow warning 不打印两份同样语义的永久段落。WARN/ERROR 不可因布局被消失。
- logs 人读输出重用同一事件投影，当前语言下重放带稳定 id 的记录；无 id 的旧记录显示「原始日志」标记并保留原文，不猜译或改写历史。
- 保留 file NDJSON writer、rotation、retention、lossy queue、tail/counters、scope sink；任何 console formatter 都不重新序列化 file JSON。

## 6. Sync 的结构化 notices 与安全确认

`SyncSummary` 只有 stats，当前 `last_error` 是混合文本，既不足以可靠翻译也不足以判 severity。方案新增 **crate-private** `RunNotice` 和可选反馈回调，原 public engine 入口以 `None` 委托同一内部实现；CLI 入口使用回调收集。不是新增 wire 事件或替换 SyncOptions。

| 语义 | 唯一事实来源/发生位置 | 命令投影 |
| --- | --- | --- |
| legacy skipped | engine 已计算 legacy source 集（`engine.rs:229-244`） | source + retained + repair action，severity 为稳定值 |
| coverage blocked | 已有 antigravity_coverage 与 SourceIssueCode（`:251-255`、`:450-497`） | missing/out-of-scope/unreadable/discovery 分支，不重扫磁盘 |
| recovery offered/choice | engine 正在调用的同一 coverage 和 callback 返回值（`:264-276`） | captured count/source/choice；确认格式在 CLI I/O adapter |
| retained hooks/history | 既有 marker/typed 状态及同轮检查结果 | 明确保留来源与既有语义，不把字符串当逻辑 |
| source failure | 当前结果/typed source issues，保留 source 与原 err cause | 本地化标题、稳定严重度、原始未知 detail |
| remote skip | 同轮 remote outcome 与现有 RemoteHostSkipped | host label/code/原始外部 detail，不误归本地 source fault |

回调只在现有决策边界发语义数据，不改变 parser 选择、事务或 cursor 顺序，不新增 per-file 查询。按本轮事实构建私有 `SyncDisplayContext`；不能在命令末尾仅靠历史 source_status 猜测用户有没有选 keep。正常结束、accepted-rebuild 失败、取消/早退都保留当前结局；无终局表的错误通过同一 formatter 呈现。

确认的读写走当前 human renderer 的完整消息/暂停恢复通道，避免 indicatif 与独立 Write 混行。提示内容见输出草案；k/r 大小写、空输入、EOF、一次非法重试后的保留、三流 TTY 和禁提示 flags 完全保持。unreadable/discovery 失败不能用确认绕过。普通 sync 仍不自动扩大 lossy rebuild 范围。

CLI sync 逻辑仍是 adapter；engine/parsers/store 不依赖 commands/i18n，不从设置读语言。TUI 的 x 同步仍用 JobRegistry，不借语言设置增加有损交互。

## 7. 统计与诊断

在 sync_summary 自己的纯 renderer 上增加 Unicode 线框。按原十列 SOURCE/FILES/CHANGED/SKIPPED/SEEN/COMMITTED/STORED/BYTES/PARSE/WRITE 投影，来源第一列左对齐、数值右对齐；外框、表头分隔、合计分隔都保留。先算 plain display width，再加颜色。仅来源 display label 可以沿用紧凑截断，数值不裁切、不换口径，不合并或删除列。

使用已有 unicode-width，参考 help/clean 的边框。不要原样复用会 shorten 数值且用 chars 计宽的 report_table；也不为了边框新增依赖或统一重构全部表格。CLI report_table 的中文 width 修正在 C 子任务局部完成。

诊断严格在 table bottom 之后，按来源顺序加标题；malformed/oversized 为 fault，skipped/accounting 为信息。counter 不是不同物理记录数。保留最多8样本、省略数量、稳定 reason code、basename/允许 offset；ZCode 时间和 OpenCode 私有 row id 保持原规则。

不把 parse/write 来源计时和当作整条命令总耗时；不改变单位、舍入或数据层计数。NO_COLOR、LLMUSAGE_NO_COLOR、非 TTY/forced-line 无 ANSI，仍有边框与文字状态。保留 stdout 最终表/诊断、stderr 过程/确认。

## 8. TUI 与 CLI 的一致性

`dash/tui` 调用共享 preferences loader；显式 home 和 store.paths.root_dir 是同一 root，TUI 内不再独立 discover。主循环用固定 locale scope 覆盖 input action/status_message 与每帧 draw；panel loader、quota 和 sync 通道返回中立值，在 UI 线程格式化。

九面板及弹窗覆盖以 `research/surface-coverage.md:T01-T10` 为准。Panel/TimeWindow/TableSortKey/SourceKind/JobStatus/theme id 保持稳定；只翻译 display label。nav_bar 绘制与 panel_at_position 必须共享同一语言标签与 Unicode cell width（当前 `nav_bar.rs:75` 用 chars，有真实点错风险）。这项是鼠标功能回归，不是新增尺寸矩阵。

帮助/来源选择/Usage 额度和 y 覆盖层全部一致；热键字母保持不变。readiness 等已有 typed state 直接投影。**当前 UsageFetchDiagnostic 没有 code，且部分 remaining_label 已把绝对数值与英文拼在一起**；T 子任务必须在现有 producer 增加 crate-private QuotaPresentationHints（中立错误种类/参数、metric身份及原绝对remaining/limit/unit），不能从百分比反推或解析英文消息。

共享内部 fetch core 提供原 public report/outcome 加 hints；public fetch_all/各公开 DTO 保持原签名/字段和值。TUI私有session携带hints并绑定report指纹，在frame scope中消费；无public AppState必填字段变化。私有CacheDocument可追加可选presentation元数据，以支持fresh-cache同样翻译；旧cache默认缺失仍可读，旧reader忽略新成员，TTL/cache_hit/请求次数不变。元数据无locale/凭据/token/原始响应，不翻译原report。这是明确的私有缓存兼容扩展，由T子任务更新subscription合同与新旧cache测试，详见research/tui-localization.md。

不改 250ms tick/dirty redraw、可见行窗口、异步 generation、排序算法、selection/hit-test 共享 shell 区域、主题/NoColor。正常窗口的中文文案可以做满足可读性的局部短标签/换行，不重设所有断点或功能布局。

## 9. 机器/嵌入兼容与明确不变项

- 保持 report JSON、diagnostics JSON、doctor/logs/catalog JSON、sync NDJSON、emit-shards/trailer、remote handshake/watermark 和 HTTP DTO 的既有 key/value 语义。locale 不进入 query/storage/remote wire。私有额度cache仅允许第8节定义的可选中立presentation元数据；原report和TTL字段不变。
- JSON 模式 stderr 可本地化；stdout 不追加菜单/日志/表格/ANSI。测试比较固定 fixture 的字段与数值，仅对原有 timestamp/run-id 等动态字段做明确规范化。
- file events 只允许可选稳定 event_code 的前向兼容追加，原 fields/message 值不随语言变；不修改历史分片。
- statusline 文本缓存改为 en/zh 分开读取/写入；无语言标记的 latest.txt 不作为跨语言 fallback，文件保留不主动删。
- Web assets/copy.js、codex-tracer browser i18n、desktop 设置不改；CLI serve/codex-tracer 的终端说明仍属于 CLI。
- 更新本轮规范中的 English-only 和 sync table 展示条款；不改 token accounting、同步协议、迁移/删除规则。update 默认 Y 与 sync 默认 keep 不统一成同一默认值。

## 10. 交付与回滚

顺序：F → S → C → T → A。S/C/T 概念上仅依赖 F（C 的 logs 展示再依赖 S），实际按该顺序串行，避免 shared catalogs/entrypoint/report_table 的交叉修改。父任务整合源需求、覆盖矩阵与最终证据，最后归档；不启动父任务写产品。

每个 child 交付后以局部门禁验证，最终 A 做跨表面 gate 与真实终端 smoke。回滚只涉及代码/展示与独立偏好；不改 schema 或重建 usage。切回英语用 lang en，不删除用户数据。没有发布/安装/真实有损重建授权。

## 实施复核补充：已知失败的中立展示信息

S 独立复核复现两处生产漏译：有损重建等安全拒绝的自有英文、OpenCode/ZCode 打开 DB 失败的自有中文。以下是原“所有自有文案本地化、公共类型及持久化不变”要求的有界实现细化，不降低验收。

- 在既有安全拒绝生产位置附加 crate-private failure kind/facts；原 Display/错误链及持久化/Failed wire 文案保持 canonical，CLI 的过程、最终错误和 run-failed console 三个展示入口按 typed 信息投影。日志仅追加必要的可选中立错误 id/参数，原 error/message 不随语言变化。
- SQLite parser 固定失败仍写原 last_error 并返回原 stats/退出结果；两个既有生产位置追加中立 reason/source 到解析 future 内的私有反馈作用域。driver 的私有结果携带 hints 给 engine notices。作用域必须隔离异步调用并自动恢复，公共 parser trait/DriveContext/SourceSyncStats/SyncEvent 不改，不绕过自定义 parser、不再次打开或扫描来源。
- parser 反馈不携带 locale/UI 字符串、不依赖 commands/i18n；不在进程全局保存可变状态。直接公共 parser 调用没有作用域时保持旧行为。测试涵盖配对实际失败、RUST_LOG=off/error、canonical JSON/错误原值及反馈隔离/数据等价。


### S 复核补充：文件清单错误

独立复现 CODEX_HOME/sessions 为普通文件时，SourceFailed 仍携带自有固定英文。允许在 source_files.rs 既有失败 producer 将“根不是目录”与外部 IO/WalkDir detail 作为中立私有事实发往同一调用 collector，由 driver 当前 source 归属后投影。保留原 errors/error_summary/last_error、退出码和发现顺序；ownership skip 不转换成故障，不增加六个 parser 的公开字段，不解析字符串或重扫。回归必须检查英中实际进程输出及 canonical 记录不变。

消费点细化：发现函数也可能只为归属或覆盖检查调用，因此不在每次 discovery 立即发射。SourceFileListing 以私有事实保留错误来源，六个内建 parser 在将 error_summary 原值赋给 last_error 的现有调用点使用私有反馈适配器，明确 source 后送入现有 collector。未知自定义 parser 不进入该路径，继续保留原始详情；不比较字符串。不增加 task-local source scope。清单错误样本沿用原 cap3 与数值 omitted，parse issue cap8 不变。

### C 实施复核：命令诊断与清理事实

批准 C 子任务 design 的命令错误日志/旧集成清理补充：Diagnostic 可携带私有封闭诊断 code 与中立事实，供同目录投影最终错误和 .03/.04 日志重放；canonical error/message/run 数据保留，未知或畸形 metadata 明示原始详情，不存英中模板。四个旧集成 producer 可经私有同调用结果传回清理结果/失败事实，保持公开 cleanup/IntegrationAction、审计及配置字节、恢复与删除集合不变。C 的两个实施者协作 shared adapter，独立检查核对事实与行为等价，不解析自由文本或增加全局反馈缓存。
