# CLI / TUI 冻结覆盖清单

用户范围决定：2026-10-20，先选择整个 CLI，随后追加 TUI；Web 仍不改。下表是实现范围和验收所有权，不是已通过清单。所有功能证据当前为 NOT_RUN。

子任务简称：F=language-foundation，C=command-output，S=sync-console，T=tui-consistency，A=terminal-acceptance。F 负责所有路径的帮助与 parse error；其余列负责运行时 human 输出。

## CLI 命令树

| ID | 完整路径/展开规则 | 运行期人读覆盖 | 机器/副作用边界 | Owner |
| --- | --- | --- | --- | --- |
| C01 | 根、无子命令、-h/--help、help、help --zh、help PATH、-V/--version | 根帮助、所有子命令/参数说明、默认 daily、参数错误；版本身份数字不变 | help/version/parse error 不建 DB/logs/home | F；默认 daily 由 C |
| C02 | lang [en\|zh\|zh-CN] | 双语可识别选单、当前语言、目标语言确认、取消/读写错误 | 非 TTY 不读 stdin；仅显式保存写独立 settings | F |
| C03 | daily、weekly、monthly、session、blocks | 标题、列名、合计、注释、empty/loading/error、所有人读分支；daily --instances、session --id、sections 同样覆盖 | --json/--no-cost/--by-agent/--sections 数值及 DTO 不变 | C |
| C04 | {claude,codex,opencode,antigravity,antigravity-ide} × {daily,weekly,monthly,session}（20 个叶路径） | focused 报表及冲突/不支持参数错误 | source 筛选、JSON、token/cost 投影不变 | C |
| C05 | statusline | today/block/projected/unavailable、cost source 标签与自有错误跟随语言 | 保持单行、原单位与数字；stdin hook JSON 不翻译；缓存按语言隔离，旧单语缓存不跨语言回退 | C |
| C06 | init | 完成结果、下一步与错误 | 被动初始化与现有 DB 规则不变 | C |
| C07 | sync；sync --rebuild；--recent-days；--source | 启动、锁、定价、source/remote 进度、确认/重试、失败/取消、十列线框表、来源诊断 | --json-events/--emit-shards 严格协议；确认与数据保护不变 | S |
| C08 | remote add/list/remove/sync | 注册/列表/保留或删除结果、拒绝、离线与错误 | remove --delete-usage 仍要求 --yes；SSH argv/水位/标识不变 | C |
| C09 | remote handshake（隐藏） | help/参数错误与 stderr 可本地化 | stdout 始终原 handshake JSON，不改键/版本/源映射 | F/S；C 验证协议 |
| C10 | status、source-status | worker lock、host/source 状态、质量、故障/保留历史、计数提示、动作 | typed source/status 枚举不改；不翻译注册标签/源 ID | C |
| C11 | diagnostics；--out；--forget-file | 写出路径确认、forget-file 结果、歧义/拒绝与错误 | 默认 stdout 与 --out 内容均原 JSON；deleted_by_user 等机器状态不翻译 | C |
| C12 | doctor；--refresh-pricing | 检查、问题与下一步、计价刷新结果 | --json 原 payload；不改健康严重度与定价 | C |
| C13 | catalog apply/status/reset | 层/状态/模型计数、apply/reset 结果、验证拒绝与错误 | status --json 不变；不改覆盖/重算算法 | C |
| C14 | clean [--yes] | 分类线框表、动作、删除结果/失败、没有内容、引导 | 默认不删；缺失 home 不创建；settings/lock 为 keep；既有删除集不扩大 | C；F 提供设置分类 |
| C15 | logs | 状态/统计、条目和 run 结果说明按当前语言；带 id 的新消息重投影 | --json 与旧记录原文不变；无 id 的历史/外部消息明确标注原始 detail，不猜译 | C；S 提供消息投影 |
| C16 | serve | URL、网络暴露/SSH/浏览器跳过、启停/失败与终端日志 | Web 文案/localStorage/API/监听策略不改 | C/S |
| C17 | dash；tui（隐藏兼容别名） | 启动/错误/弃用提示；界面见 T 清单 | raw-mode 清理、快捷键与 JobRegistry 不变 | C/F 启动；T 界面 |
| C18 | export html | 导出完成/路径、错误与日志 | 导出的 HTML/JS/snapshot/Web 语言保持原行为 | C/S |
| C19 | uninstall [--purge] | 遗留清理动作、noop/failure、purge 结果与错误 | 不扩大所有权/删除范围；purge 原语义不变 | C |
| C20 | update [main\|dev] [--check] | 预览、官方源/版本说明、确认/重试/取消/目标改变/结果 | 默认 Y、y/yes/n/no、EOF失败、二次 ref 校验不变；Cargo/git 原生输出不翻译；测试 fake provider/executor | C |
| C21 | codex-tracer | CLI 解析/重建统计、服务器 URL、错误与日志 | 浏览器 dashboard i18n、其数据/HTTP协议不改 | C/S |
| C22 | 全路径 stderr/RUST_LOG 等级输出 | app-owned trace/debug/info/warn/error 使用消息 id，CLI 范围内统一语言与格式 | 结构化文件日志 level/filter/原 message 不随 locale 改；只允许新增可选 message id 字段 | S |

当前 `Commands` enum 有 28 个分支（`src/commands/mod.rs:56-224`），含隐藏 tui；新增 lang 后 29 个。覆盖测试从 Clap 元数据枚举容器和叶路径，自动展开 C04，不把上表行数误算为命令数。新增/删除命令必须改变覆盖清单，不能沿用「27 个含 print 宏文件」作为完整性证明。

## TUI

| ID | 表面 | 必测内容 | 保持的语义 |
| --- | --- | --- | --- |
| T01 | 导航、footer、公共状态、时间窗口 | 9 个长/短标题，状态/过滤/自动刷新/加载提示，快捷键说明 | panel enum/index、热键字母、可排序 key 不翻译；中文按显示宽度命中正确页签 |
| T02 | Overview | 图例、时间/指标说明、列表/empty/error/loading | 原图/模型排序与可见行窗口 |
| T03 | Usage（Panel::Trends） | 摘要、账户/额度、ready/watch/critical、认证/缓存/刷新/重置时间、隐藏邮箱、provider diagnostics | 外部 provider/model/账户值不翻译；凭据只读、TTL、r/R 与 m 的行为不变 |
| T04 | Models | 表头、价格/缓存/筛选说明、空错加载、排序箭头 | 用稳定 TableSortKey 排序，数值和行集合相同 |
| T05 | Daily、Hourly、Monthly | 每个面板表头/周期说明/空错加载，Daily/Monthly Enter 明细和返回 | 时区/分组/选中行/滚动不变，中文列宽不盖住数值 |
| T06 | Stats | 年热力图、统计项、Day Breakdown、empty/error、选择/返回提示 | 日期命中与共享 dashboard_shell_areas、原统计口径 |
| T07 | Agents / Behavior | 域标签、工具/活动/质量说明、限制/无数据/错误 | model/tool/source 名及原行为数据不改 |
| T08 | Blocks | 表头、活跃/结束、预测/限额说明、空错加载 | 投影与燃烧速率算法不变 |
| T09 | Help/Settings、Source Picker | 全部帮助说明、主题/来源/窗口标签、全部来源/monitor-only 状态/操作提示 | 主题 id、s/a/t/Enter/Esc/q 等热键不变；不增加 TUI 语言菜单 |
| T10 | y 同步覆盖层、x 同步反馈 | Source Sync/Platform Monitor、阶段、故障/取消/完成/重试、accounting 状态 | in-process JobRegistry；不调用 commands::sync，不自动接受有损重建 |

来源依据：`src/tui/app.rs:78-104`、`:376-445`；`src/tui/draw.rs:25-110`；`src/tui/nav_bar.rs:55-80`；`src/tui/help_dialog.rs:10-78`；`src/tui/sync_control.rs:38-61`；`src/tui/panels/usage.rs`。

## 冻结的语言边界

- 两种语言是 en 与 zh（简体）；zh-CN 仅规范化别名。命令/flags、enum wire values、reason/code、数值单位、用户数据保持原值。
- lang 保存之后，新 CLI 进程与新启动的 dash/tui 一致；正在运行的 TUI 固定启动时快照，退出重开生效。不做文件轮询或跨进程热切换。
- statusline 是 CLI 人读结果，必须翻译自有标签；其单行缓存不能回放另一语言文本。
- --locale 继续仅保留既有报表字段/解析值 en-US/zh-CN/ja-JP；lang 控制界面文字，--locale 不反向覆盖它，不新增日语 UI。当前 query/render 并未消费该字段生成语言化数字，不借机添加数字文化格式功能。
- HTTP/API、NDJSON/JSON、持久化诊断/原始错误/旧日志、导出 Web 内容和第三方子进程内容属于受保护数据。新 app-owned 人读上下文仍必须翻译，不得把所有遗漏文案都归到「原始 detail」。
- `tui-presentation-contracts.md:46` 的 English-only 要在 T 子任务按用户本轮明确决定修订；这是规范更新项，当前规划不修改规范。
