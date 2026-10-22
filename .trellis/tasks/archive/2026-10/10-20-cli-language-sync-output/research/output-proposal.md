# 双语输出示意（设计参考，非运行截图）

这里只展示候选交互语义和信息层次。表格使用虚构小型 fixture，避免把用户真实同步视作本轮验证；完整示例的十列不删减。

## 语言入口

语言入口行为见已完成设计；以下是人读示意：

```text
llmusage lang          # TTY 选择；非 TTY 显示当前语言和显式设置方法，不读 stdin
llmusage lang en       # 持久化英语
llmusage lang zh       # 持久化中文（简体）
```

选择页始终让两种语言可识别：`English`、`中文（简体）`。例如通过 1/2 选择，显示当前值，空输入/取消/EOF 不修改。成功确认使用目标语言；lang 不要求 init、不打开用量 DB、不获取 worker lock。命令名、参数键及 en/zh 代码不翻译。

设置存储：当前 CLI runtime home 下 `cli-settings.json`。配置缺失视为 en；不根据系统语言改变这个默认值。文件损坏回退英语并说明修复入口。CLI 语言与既有报表 `--locale` 分开，后者的兼容语义需明确写进最终 design。

## 过程信息

英文候选：

```text
[INFO] Database  Worker lock acquired · waited 27ms
[INFO] Pricing   Recomputing event costs · 70,000/399,560 (17.5%) · elapsed 31.1s
[WARN] Pricing   Recompute is taking longer; work is still progressing.
[WARN] History   antigravity uses legacy accounting; history is kept pending recovery choice.
```

中文候选：

```text
[信息] 数据库  已获取同步锁 · 等待 27ms
[信息] 定价    正在重算事件成本 · 70,000/399,560（17.5%）· 已用时 31.1s
[警告] 定价    重算耗时较长，任务仍在推进。
[警告] 历史    antigravity 使用旧版计数；选择恢复方案前保留历史。
```

- 标签和阶段词本地化，source id 保留。
- 行输出与进度条共用信息含义，但不要求把每个进度 tick 变成永久行；不产生重复的「每源完成」段落。
- 这里只给出信息结构，不承诺秒级 ETA；长版本哈希从默认人读行移除，完整版本身份继续存在于结构化日志或显式详细信息。
- `[WARN]` 的既有慢任务级别不悄悄改成 INFO；普通进度与慢任务解释分清。真实失败还须显示原因与可执行动作。
- 可用颜色加强层级，但去掉所有颜色后仍能辨别状态。

## 高风险恢复确认

英文候选（使用用户样例里的计数，仅用于文案讨论）：

```text
Recovery required · antigravity_ide
  Preserved history: 9,591 events
  Missing inputs:   501 tracked databases
  New inputs:       45 files
  Rebuilding discards history from missing databases.

Keep history [k] / Rebuild and accept data loss [r] (default: keep):
```

中文候选：

```text
需要选择恢复方案 · antigravity_ide
  保留的历史：9,591 条事件
  缺失输入：  501 个已跟踪数据库
  新增输入：  45 个文件
  重建会丢弃缺失数据库对应的历史记录。

保留历史 [k] / 重建并接受数据丢失 [r]（默认：保留）：
```

`out_of_scope_count > 0` 必须单独说明「输入仍存在，但不在当前发现范围；只保留当前可解析文件重建的结果」，不能误译为文件已被删除。不可读/发现失败不能提供能绕过保护的确认选项。

保留现有输入语义：k/r 均不区分大小写；空输入、EOF、读失败保持历史；第一次非法输入按所选语言重试一次，其后安全返回 keep。三流 TTY 条件与禁止提示的 flag 组合保持不变。

## 完成表格

英文虚构 fixture：

```text
Sync finished:
┌────────┬───────┬─────────┬─────────┬──────┬───────────┬────────┬────────┬───────┬───────┐
│ SOURCE │ FILES │ CHANGED │ SKIPPED │ SEEN │ COMMITTED │ STORED │ BYTES  │ PARSE │ WRITE │
├────────┼───────┼─────────┼─────────┼──────┼───────────┼────────┼────────┼───────┼───────┤
│ codex  │     2 │       1 │       1 │   12 │         8 │    120 │ 2.0 MB │ 100ms │ 200ms │
│ claude │     3 │       2 │       1 │   20 │        10 │    200 │ 3.0 MB │ 200ms │ 300ms │
├────────┼───────┼─────────┼─────────┼──────┼───────────┼────────┼────────┼───────┼───────┤
│ TOTAL  │     5 │       3 │       2 │   32 │        18 │    320 │ 5.0 MB │ 300ms │ 500ms │
└────────┴───────┴─────────┴─────────┴──────┴───────────┴────────┴────────┴───────┴───────┘
```

对应中文表头候选：

| 原列 | 中文 | 不变的数据含义 |
| --- | --- | --- |
| SOURCE | 来源 | stable source id，不把 antigravity_ide 改成命令别名 |
| FILES | 文件 | 本轮检查的候选文件/数据库，不是已写入文件 |
| CHANGED | 变更 | 需要解析或增量扫描的文件/数据库 |
| SKIPPED | 跳过 | 无需重解析的文件，不是跳过记录数 |
| SEEN | 读取事件 | 去重前观察到的规范化事件 |
| COMMITTED | 新增事件 | SQLite 去重后本轮插入事件，保持 events_inserted 的既有含义 |
| STORED | 库存事件 | 本轮结束该来源的已存事件，不是本轮新增 |
| BYTES | 读取量 | 现有逻辑读取字节指标，不宣称实际磁盘 I/O |
| PARSE | 解析 | 既有 parse_ms，不宣称 CPU 时间 |
| WRITE | 写入 | 既有 write_ms 边界，不扩大事务计时范围 |
| TOTAL | 合计 | 对来源各列饱和求和，不用来替代整条命令耗时 |

中文表格使用同一边框算法及显示宽度。`B/KB/MB/GB/ms/s` 为稳定单位记号，暂不增加数字文化格式或单位换算功能。缺失来源保留既有占位，不能显示成实际观测到的 0。

表后诊断候选：

```text
Diagnostics · grok
  Accounting information: 14 usage-completeness observations
  Samples shown: 8/14 · 6 not shown
    usage_incomplete · offset 3983970 · updates.jsonl
```

```text
诊断 · grok
  计数提示：14 条用量完整性提示
  展示样本：8/14 · 另有 6 条未展示
    用量不完整（usage_incomplete）· 偏移 3983970 · updates.jsonl
```

此处 accounting 是信息项，不能把它重命名为「14 个解析失败」；一条物理记录可能产生多项 accounting 提示，也不能宣称是 14 条不同记录。真实 malformed/oversized 仍独立显示。样例不公开完整路径、path_hash、prompt、响应或原始记录。

## TUI 一致性

同root的lang zh/en在下次dash/tui启动生效；九面板、nav/footer、Help/Settings、SourcePicker和x/y同步反馈使用同一偏好。保持1-9/tab/j/k/o/O/s/r/R/x/y/m/t/?/q等快捷键不变，不新增TUI语言菜单或文件轮询。Web/browser dashboard继续独立。

例如Overview/Usage/Models/Daily/Hourly/Monthly/Stats/Agents/Blocks的中文显示为概览/额度/模型/每日/每小时/每月/统计/智能体/时段；具体短标签必须按原功能含义与display width验收，不翻译Panel enum。nav绘制和鼠标命中必须共享标签与宽度。完整表面清单见surface-coverage.md；额度rawreport与private presentation hints机制见tui-localization.md，不能从百分比臆算绝对额度。
