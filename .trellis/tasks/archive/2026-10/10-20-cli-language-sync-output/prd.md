> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# CLI / TUI 双语设置与同步输出优化

## 目标与用户价值

让用户通过 `llmusage lang` 选择英语或简体中文，默认英语。整个 CLI 的帮助、参数错误、日志、交互选项、各命令结果，以及 dash/tui 全界面使用同一语言设置。同步过程清楚表达阶段、进度、风险与动作，完成统计呈现为带完整线框的真实文本表格。

## 已确认决策与授权

- 2026-10-20 初始请求：深入分析并创建优化 Trellis 任务。
- 用户随后明确选择整个 CLI，覆盖帮助、参数错误、日志、交互与结果；之后追加「TUI的语言也和这里保持一致」。**当前范围是 CLI + TUI，Web 暂不改。**
- 用户明确说「继续完善设计、验收标准和子任务，先不要实现」。本轮只规划，父子任务全部 planning；禁止 task.py start、产品改动、真实 sync/rebuild、安装、提交、推送或发布。
- 产品基线为 dev@2b70e3f；本轮规划不改变代码。原始 sync 输出是用户提供的问题证据，不是本轮复测结果。
- 语言代码 en/zh，zh-CN 规范化别名；配置跟随当前 root。TUI 在启动时读取一次，运行中不增加语言选择/轮询；退出重开应用新设置。Web/desktop/browser dashboard 语言保持独立。
- 具体命令树、边界与交付归属冻结于 `research/surface-coverage.md`，不再保留仅 sync 的备选范围。

## 背景与仓库证据

| 已确认事实 | 源码锚点 | 含义 |
| --- | --- | --- |
| 进度中文、统计与恢复确认英文，各自硬编码 | `src/commands/sync_progress.rs:488-546`、`:362-381`；`src/commands/sync_summary.rs:18-32`、`:70`；`src/parsers/antigravity.rs:406-468` | 需统一全链文案与展示边界 |
| 顶层帮助早于 parse/logging，home 为 global | `src/lib.rs:99-117`；`src/commands/mod.rs:45-47` | 在参数错误发生前只读解决语言与 root |
| 根命令28分支，focused还展开20条路径 | `src/commands/mod.rs:56-224`；`src/commands/focused.rs:32-42` | 必须用元数据覆盖全部路径，不能只清点含 print 文件 |
| console/file 两层，慢 pricing WARN已有进度 | `src/runtime/logging.rs:264-301`；`src/store/mod.rs:983-986`、`:1023-1065` | 优化终端说明但保留结构化字段、级别与 filters |
| summary 用英文子串判级别、字符数算宽 | `src/commands/sync_summary.rs:90-100`、`:108-127`、`:181-189` | 翻译前解除自然语言与状态逻辑耦合 |
| TUI有九面板、三个dialog、同步/额度异步反馈 | `src/tui/draw.rs:25-110`；`src/tui/app.rs:376-445`；`src/tui/sync_control.rs:38-61` | 全部人读状态须覆盖，工作线程/DTO保持中立 |
| 中文导航会影响点击位置 | `src/tui/nav_bar.rs:55-80` | 绘制与命中使用相同本地化标签和显示宽度 |
| 现行TUI规范要求English-only | `.trellis/spec/llmusage/backend/tui-presentation-contracts.md:46` | 依据用户最新明确决定在T子任务修订该条，当前不改规范 |

更多证据见 `research/repository-analysis.md` 和 `research/cli-startup-errors.md`。原样例 70,000/399,560 约17.5%，31.1s慢提示不能被解读成卡死；PARSE/WRITE之和也不能冒充命令墙钟时间。重建前后库存差值不能直接当作精确丢失数。

## 需求

| ID | 优先级 | 用户可观察结果 |
| --- | --- | --- |
| R1 | P1 | 缺失设置时英语；中文选择跨进程保存，可切回英语；显式 home/profile 隔离，不由系统语言覆盖。 |
| R2 | P1 | lang提供双语可识别选择与显式 en/zh 设置；无需init/DB/worker lock；非TTY不读stdin；取消不保存，失败不误报成功、不截断旧配置。 |
| R3 | P1 | C01-C22全部CLI自有帮助、框架/约束错误、人读结果、交互及日志遵循语言，包括hidden alias/协议命令的帮助和stderr。参数/enum/code/用户值不翻译。 |
| R4 | P1 | sync恢复提示完整说明来源、保存事件、missing/out-of-scope/unreadable、当前输入、重建后果，保留现有默认keep与输入/TTY/flag安全规则。 |
| R5 | P2 | 日志突出状态、阶段、关键进度及必要动作，默认减少长哈希/内部字段堆积；真实WARN/ERROR与未知错误不隐藏，详细记录仍完整。 |
| R6 | P2 | 完成表有外框、竖线、表头与合计分隔；原十列、来源顺序、占位、数值精度/合计不变，中文正确对齐，数值不裁剪。 |
| R7 | P2 | 表后诊断按来源组织，fault与skipped/accounting信息分开，保留稳定code、最多8样本、省略数与隐私规则；不把提示数说成失败/不同记录数。 |
| R8 | P1 | 保持机器数据/协议、公开类型、日志存储、业务状态/退出码、token/query/sync/重建/删除行为和共享消费者兼容。 |
| R9 | P2 | 双语README及CLI/first-sync/TUI指南说明默认语言、lang、作用域与边界；受影响规范更新，真实验证状态清楚。 |
| R10 | P1 | dash/tui读取与CLI相同root偏好；九面板、导航/footer、帮助/设置、来源选择、y覆盖层、x反馈、quota与加载/空/错/取消状态全都跟随，默认英语。 |
| R11 | P1 | TUI本地化后热键/选中/排序/鼠标命中仍正确，原dirty redraw、visible-row window、异步数据/JobRegistry、quota凭据/缓存/刷新/邮箱隐藏、主题/NoColor不退化。 |

## 兼容与边界

- 机器消费内容不翻译：JSON/NDJSON/SSH/HTTP的key、code、枚举值和数值，诊断文件、订阅缓存和导出的Web bundle保持原契约。JSON模式stderr可本地化但stdout不得污染。
- 第三方Cargo/git/OS/SQLite原始详情和缺稳定id的历史日志允许保留原文，配本地化上下文/标记；不得以此豁免新app-owned文案。历史日志不重写，file event只允许新增可选稳定id，不让language改变原message/level/fields。
- statusline为CLI人读结果，标签应翻译、保持单行并隔离各语言缓存；stdin hook JSON/模型/成本等数据不翻译。
- --locale继续原报表参数兼容语义，不控制CLI/TUI文案，不新增日语UI或数字文化格式。
- 不修改公开AppContext/AppPaths/SyncSummary/SourceSyncStats/错误类型的必填字段或旧调用签名。领域层不读取语言，不反向依赖commands。
- 读取设置不创建home/logs/DB；clean继续忽略LLMUSAGE_HOME且缺失root不落盘。新settings及其锁属于keep，不扩大删除集。
- 同步确认与update确认各保留原默认值：sync默认keep；update空输入默认yes、EOF失败、二次ref校验保持。不改成统一确认策略。
- accounting与parser/cursor/token/计价/重建算法不改。PARSE/WRITE、BYTES维持原有计量边界。
- TUI只在启动应用语言快照；不增加运行时菜单/热切换，不改变快捷键或重做界面布局。
- 额度诊断当前无code且部分余额为英文String；允许私有cache新增可选、语言中立的presentation元数据，原publicreport/DTO、fetched_at/TTL/cache_hit不变，新旧cache都可读。此明确兼容扩展必须有live/cache/hints错配测试，不能从百分比造绝对数。

## 验收标准

| ID | 需求 | 必须通过的可观察检查 | Owner |
| --- | --- | --- | --- |
| AC1 | R1 | temp profile无设置英语；lang zh后新CLI/TUI中文，lang en后都英语；显式home/env/默认root隔离。 | F/T/A |
| AC2 | R2 | lang不要求初始化、不等worker lock；TTY菜单双语可识别，非TTY不读stdin；空/取消/EOF不保存，坏code/写失败无旧配置破坏。 | F |
| AC3 | R3 | 遍历Clap所有路径和ArgId，根/子/嵌套/hidden帮助、框架标题/可选值/默认值及错误有成对模板；help/version/参数错误保持原通道/退出码。 | F/A |
| AC4 | R3 | C01-C22逐项具备英中成功/空/拒绝/失败结果证据；20个focused路径、statusline、JSON-only分支、serve/codex-tracer终端说明无遗漏。 | C/S/A |
| AC5 | R4 | 两语言覆盖k/K/r/R、空、EOF、读失败、非法重试；三流TTY及json/rebuild/recent-days禁止提示不变；unreadable/discovery不能绕过保护。 | S |
| AC6 | R5 | pricing/legacy/全CLI app-owned trace到error日志按id投影；默认行无完整哈希堆积，详细/文件保留完整身份与原等级；未知/历史detail仍可诊断。 | S/C |
| AC7 | R6 | 同fixture两语言十列闭合边框、header/total分隔、Unicode对齐；行序/数值/单位/饱和合计/缺失占位不变，大数值不裁剪。 | S |
| AC8 | R7 | 诊断来源清楚，malformed/oversized与accounting/skipped分开，8/14与6省略等计算准确，basename/offset/ZCode/OpenCode隐私规则不变。 | S/C/T |
| AC9 | R8 | 两语言report/doctor/logs/catalog/diagnostics JSON、syncNDJSON、shards/handshake等协议字段与固定值一致；只对明确动态字段规范化，stdout无菜单/日志/ANSI。 | S/C/A |
| AC10 | R8 | human最终表/诊断stdout、过程/恢复stderr；进度警告不串行，早退/失败/取消恢复sink；NO_COLOR/非TTY/forced-line无ANSI。 | S/A |
| AC11 | R1-R2/R8 | 无home的读取/help/version/参数错误不建DB/logs/root；clean例外与settings keep分类通过；中英并行/嵌套scope不串语言。 | F/C |
| AC12 | R9 | 双语README/reference（含dash/TUI章节）/commands/first-sync与新合同一致；Web/desktop语言不受影响，TUI旧English-only条款按本轮决定改写。 | A/T |
| AC13 | R10 | T01-T10所有面板/dialog/覆盖层的data/loading/empty/error与同步/额度状态两语言TestBackend覆盖；quota live/fresh-cache/new-old reader/hints错配通过，原DTO/绝对数/TTL不变；dash与tui一致。 | T |
| AC14 | R11 | 同数据/语言的nav绘制与点击命中相同Panel；j/k/o/O/Enter/Esc等输入、sort/selection、idle ticks/visible rows、quota r/R/m/TTL/凭据只读和JobRegistry取消回归通过。 | T |
| AC15 | R3/R8 | statusline语言缓存不串，update/remote/clean/uninstall等确认/删除/第三方执行规则不变；所有测试mock外部调用，不操作真实用户数据。 | C |
| AC16 | R1-R11 | 对应cli/sync/tui/remote/architecture与既有store/logging测试通过，跨Rust+docs最终just ci通过；普通窗口真实CLI/TUI smoke独立记录PASS/FAIL/NOT_RUN，不拿headless冒充native。 | A |

完整测试机制见 `research/validation-plan.md`。本轮所有产品测试与native验收均NOT_RUN，规划结构校验另行记录。

## 父子任务与交付映射

| 子任务 | 交付 | 对应需求 | 前置 |
| --- | --- | --- | --- |
| 10-20-cli-language-foundation | Language/settings/lang、scope、启动、help/Clap/final errors | R1-R3、R8 | 无 |
| 10-20-cli-language-sync-console | 日志id/formatter、sync notices/确认/表格/诊断 | R3-R8 | F |
| 10-20-cli-language-command-output | C清单其余命令人读迁移、report_table、statusline缓存 | R3、R8 | F；logs展示依赖S |
| 10-20-tui-language-consistency | T清单全部UI与交互语言/功能一致性 | R10-R11、R7-R8 | F |
| 10-20-terminal-language-acceptance | 文档/合同与跨子任务证据整合 | R9、R1-R11最终验收 | F/S/C/T |

计划串行F→S→C→T→A；父任务拥有需求/整合，不当作实现target。依赖必须写在子任务文件，parent/child不是调度器。

## 范围外与风险

范围外：Web/desktop/browser dashboard翻译联动、第三语言/在线翻译、运行中语言热切换、token/SQL/计价/同步性能优化、数据恢复/真实重建、全部表格重构、尺寸/DPI/断点矩阵、版本提升/安装/发布/提交。

需要重点控制：早期home识别不改变Clap接受语法；typed notice不破坏wire/public类型；新应用文案不能逃进raw fallback；中文显示宽度与TUI命中一致；保存偏好不争用DB；共享renderer与日志不得影响Web/嵌入调用。机制、回滚与所有权详见design/implement。

## 规划状态

用户拥有的范围决定已经解决，没有阻塞产品问题。prd/design/implement和五个子任务应保持一致，完成context与父子关系校验后交付审阅。用户「先不要实现」限制仍有效；除非之后明确解除并批准最新规划，不运行task.py start。规划完成不等于功能完成。
