# 仓库分析：CLI 双语设置与同步输出

研究日期：2026-10-20。依据当前 `dev` 分支，起点 `2b70e3f`；本轮只读产品代码，不运行真实 sync/rebuild，不读取用户 usage 数据。用户粘贴输出是问题样本，不是本轮复测结果。

## 1. 问题重述与优先级

真正需求是让用户能选择一种稳定的终端语言，并迅速分辨「正在做什么、是否需要处理、最终写入多少」。不是单独给英文表格换标题，也不是给现有 tracing 文本套一个边框。

| 优先级 | 当前问题 | 用户影响 |
| --- | --- | --- |
| P1 | 没有统一 CLI 语言设置；进度、日志、确认和统计各自写死语言 | 一次 sync 混用中英；新增消息会继续漂移 |
| P1 | 重建确认属于数据丢失决策，不能只翻译 keep/rebuild 两个词 | 必须保留源、历史条数、缺失/范围外输入、丢失后果和安全默认值 |
| P1 | 本地化若改到领域错误、持久化诊断或 JSON 事件 | CLI、桌面、Web、SSH 消费者可能被连带改变 |
| P2 | 默认 stderr 暴露长版本哈希与结构化调试字段 | 关键信息被版本身份淹没，慢任务容易被误读为失败 |
| P2 | 完成统计只有空格对齐；诊断缺少独立来源标题 | 列与诊断归属难扫读 |
| P2 | 统计宽度按字符数而非终端显示宽度计算 | 换成中文表头后竖线可能错位 |

样例中的定价重算 `70,000 / 399,560` 约为 `17.5%`，耗时 `31.1s`。代码证明 30 秒阈值触发一次 WARN，并不证明卡死。`PARSE + WRITE` 是来源指标之和，不能冒充整条命令墙钟耗时。

样例中的 `3,477 → 2,585`、`9,591 → 1,639` 分别是确认前保存历史与重建后的库存；重建同时可能导入新事件，不能把差值直接宣称为精确丢失事件数。本任务不推断已删除数据，也不尝试恢复数据。

## 2. 输出链与证据

以下行号均来自本轮源码读取。

| 输出面 | 当前实现与证据 | 对规划的约束 |
| --- | --- | --- |
| 命令入口 | `src/lib.rs:99-117` 先拦截顶层帮助，再 `Cli::parse()`，之后才发现路径并初始化日志 | 语言必须早于首条人读输出生效；不能只在 sync dispatch 中读取 |
| CLI 定义 | `src/commands/mod.rs:38-52` 根说明为中文，`--home` 为 global 参数；`src/commands/mod.rs:56-232` 为命令枚举 | 若范围为全 CLI，还必须覆盖 Clap 子命令帮助及参数错误，而非仅手写帮助 |
| 已有双语帮助 | `src/commands/help.rs:7-10`、`:43-108` 有 English/Chinese 文案；`:112-129` 只识别 `-h/--help/help/help --zh` | 可复用文案；目前固定英文入口与显式中文入口独立，不是统一 locale 系统 |
| 路径 | `src/runtime/paths.rs:35-73`、`src/runtime/app.rs:23-38` 支持显式 home 与环境发现 | 配置应跟随当前 CLI home，隔离 profile，不放 SQLite |
| clean 例外 | `src/lib.rs:105-114`、`runtime-log-contracts.md` 的 missing-home 场景 | `clean` 不读 `LLMUSAGE_HOME`、不创建 home/logs；语言加载不能破坏它 |
| 人读 sync | `src/commands/sync.rs:252-292` 创建 renderer、sink 和 bootstrap 回调 | 保留 guard 和完整行路由；禁止再开独立 stderr 输出链 |
| 进度文案 | `src/commands/sync_progress.rs:488-546`；`:362-381` 另有进度条专用中文 | line 与 bar 均需覆盖，翻译 `human_progress_line` 一处不够 |
| tracing | `src/runtime/logging.rs:264-301` console fmt 与 file JSON 两层，各有 filter | 终端格式器在 event/field 层工作；不能对已序列化 NDJSON 或 ANSI 字符串做替换 |
| 慢重算 | `src/store/mod.rs:983-986`、`:1023-1065`、`:1086-1120` | 1 秒/25k 进度事件与一次慢 WARN 已存在；保持 file WARN 与 operation/phase，不重写定价算法 |
| legacy 警告 | `src/sync/engine.rs:229-244` | 先记录跳过，再可能进入用户明确授权的恢复流程；终端时态应明确「当前保留，待选择」，避免与后续 rebuild 相矛盾 |
| 重建确认 | `src/commands/sync.rs:170-177`、`:227-240`；`src/parsers/antigravity.rs:406-468` | 仅满足三流 TTY 且无 json/rebuild/recent-days 才读 stdin；空输入/EOF/读失败保留，非法输入只重试一次，k/r 大小写语义不变 |
| 保存提示 | `src/parsers/antigravity.rs:325-403` 与 `src/sync/engine.rs:450-497` | English notice 进入 `last_error`/accounting_warning，不应直接把本地化后的 UI 字串写回数据库 |
| 统计投影 | `src/commands/sync_summary.rs:18-32`、`:53-105`、`:132-156` | 十列、来源顺序、按 source 饱和求和、缺失来源占位与合计必须保留 |
| 隐藏脆弱点 | `src/commands/sync_summary.rs:90-100` | 当前 `error.contains(...)` 判断警告颜色；翻译后不能继续靠自然语言子串判严重度 |
| 中文显示宽度 | `src/commands/sync_summary.rs:108-127`、`:181-189`、`:399-410` | `.chars().count()` 必须在本表改成 display width；不顺带重构所有报表 |
| 样本说明 | `src/domain/models.rs:118-143`、`:181-196`，`src/commands/sync_summary.rs:201-255` | 人读文案在 CLI 适配层翻译；reason/code 保持定位能力，样本最多 8 个，只显示 basename 与允许的 offset |
| 来源故障 | `src/domain/source_diagnostics.rs:13-35`、`:45-52`、`:69-105` | 已有闭集 code/count/scope/observed_at；优先读取结构化事实，不正则解析 last_error |
| 可读诊断事实 | `src/store/sync_status.rs:174-195` | 可在命令结束后按 host 一次加载 source issues；需要配合本轮结果、窗口与确认选择防止使用旧诊断 |
| 兼容表面 | `src/sync/types.rs:146-164` | `SyncSummary` 为公开 struct，只有 stats/总数，没有 notices 字段；不得为了 CLI 渲染直接新增必填公开字段破坏 struct literals |
| 报表 locale | `src/commands/report_args.rs:54-56`、`:354-358` | `--locale` 接受 en-US/zh-CN/ja-JP，是既有报表参数；不等于全局 CLI 语言，不能静默改义 |
| 现有表格参考 | `src/commands/help.rs:175-202`、`:281`；`src/commands/clean.rs:447-488`、`:666` | 已有线框与 Unicode width；无需为了 sync 表格新增依赖 |
| 报表渲染限制 | `src/tui/report_table.rs:993-1015`、`:1127` | 边框实现可参考，但通用 renderer 会截断 cell，且宽度同样用 chars；不能原样拿来渲染必须完整的同步数值 |
| CLI settings 分类 | `src/commands/clean.rs:193-215` | 新增 settings 文件若在 home 根目录，应被识别为保留配置，不误导为 unknown，也不得扩大 clean 删除范围 |

## 3. 这不是一个只改两个文件的小任务

对 `src/commands/**/*.rs` 中 print/eprint/tracing 宏做静态词法清点：27 个文件、189 个宏站点。包含测试/helper，不是唯一文案数，也不涵盖所有 `format!`、Clap doc comments 或领域错误。

已涉及的命令类别包括报表（daily/weekly/monthly/session/blocks/focused）、init/status/source-status/doctor/diagnostics、catalog/clean/logs、remote/update/uninstall/serve/export、statusline、codex-tracer。全 CLI 范围必须列清 human 与 machine 分支，并单独决定 shell statusline 等消费表面，不能用全局搜索替换假装完成。

Web：`src/web/assets/copy.js` 已有浏览器持久化 locale。Codex tracer dashboard 另有 `dashboard_i18n.js`。交互式 TUI 的现行规范明确 English（`.trellis/spec/llmusage/backend/tui-presentation-contracts.md:46`）。用户最新明确要求 TUI 跟随 CLI，因此将修订 English-only 条款；Web 与 browser dashboard 仍独立。

## 4. 初轮技术分析（范围已确认 CLI + TUI）

以下记录初轮研究依据。最终机制已收敛于父 design.md，尤其以其启动选择、私有 RunNotice、quota presentation hints、scope 与子任务所有权为准。

### 语言基础与启动

- 使用小型闭集 Language 与编译期双语目录；对每个 app-owned key 提供 en/zh 配对，参数用类型化值格式化。无需通用多语言平台、运行时下载语言包或第三方 i18n 框架。
- 候选文件为当前 home 下 `cli-settings.json`，只保存语言；使用现有 serde/serde_json。读取不创建目录，不打开/迁移 usage DB，不等待 sync worker lock。设置成功后才报告已生效。
- 无设置默认英语，不由操作系统 locale 自动切换。候选代码为 `en`/`zh`，`zh-CN` 可规范化为 `zh`。
- 读取损坏/不支持配置时回退英语并给出一次可操作提示，不自动覆盖原文件；显式设置失败应非零退出、保留旧文件。
- 启动需要在 parse/help/logging 前解决语言，但不能写手工不完整的第二套 CLI 解析器。全 CLI 范围应通过 Clap command builder 的结构化帮助/错误接口实现；保留 `help --zh` 的显式兼容入口。
- 现有 `AppContext` 和 `AppPaths` 为公开结构，避免新增必填字段；使用 CLI 私有上下文或兼容入口，纯 renderer 显式接收 Language，避免测试互相污染的可变全局 locale。
- 设置文件写入必须完整替换；`src/integrations/atomic.rs:478-490` 有现成 writer 可评估复用，但它带第三方配置恢复语义。先评估所有权/恢复约束，不能为写一项偏好复制一套大型事务或无关重构。

### 终端日志

- human renderer 与 console formatter 共用消息目录/显示约定：状态、阶段、关键指标和动作。已知 pricing 事件用现有 `operation + phase`，其他必要的应用事件可加内部稳定标识；禁止按中文/英文 message 文本匹配本地化。
- console 对人整理字段和文案；结构化日志保留原 level、target、时间、operation/phase、版本和计数。语言设置不重写历史日志，也不改变文件过滤/轮转/保留/丢弃计数。
- 默认终端不反复展示完整 public-<hash>；确有需要的版本身份在详细输出/文件中完整保留。默认控制台仍显示真实慢任务提示、保留历史警告和失败原因，不能简单过滤全部 WARN 来让输出变干净。
- 正常进度通过既有 SyncEvent 呈现，不依赖把 `RUST_LOG` 改成 info。JSON 模式 stderr 可本地化，但 stdout 继续仅 NDJSON。
- 未识别的第三方/底层错误不能丢弃；保持原始技术 detail，并本地化应用自有上下文。不承诺翻译外部 OS/SQLite/Cargo 错误文本。

### 恢复与诊断

- 将交互确认的格式化/输入适配放在 CLI 边界；保留已有覆盖检查与恢复选择 callback。sync/parser/store 不依赖 `commands` 或读取 CLI 语言设置。
- 用既有 typed source issues、accounting version、coverage 和本轮 choice 形成 CLI 私有 display model。`last_error` 只作为原始未知 detail，不能作为语言或警告级别判据。
- `SyncSummary` 没有这些结构化事实；在最终 design 收敛前必须明确每项 notice 的取值位置和取消/失败行为。可优先用已有 store 查询和命令本轮状态，不能把缺字段问题藏在「之后翻译」里。
- parse kind/reason 用「本地化说明 + 稳定 code」展示；未知 reason 保留已有消毒后的值。informational accounting/skipped 与 malformed/oversized 不能混成失败数。
- 每段表后诊断带来源标题。样本上限、未展示数、basename-only、OpenCode 不公开 row id、无路径哈希/原文等规则不变。

### 表格

- 直接改造 sync_summary 自己的纯渲染函数，参考 help/clean 的边框和 unicode-width；避免拉入可配置表格框架或重构所有报表。
- 保留十列与准确数值；表头、合计、本地化占位说明来自同一目录。先计算纯文本显示宽度，再加 ANSI。
- 上/下框、表头分隔、竖线、合计前分隔组成完整表格；诊断独立置于表后。
- 最小回归：英文、中文、零值/缺失来源、大整数、无颜色/非 TTY、既有紧凑逻辑。不会建立终端尺寸/DPI/断点测试矩阵。

## 5. 兼容与约束

- 保持 `SyncEvent`、`SyncSummaryEvent`、source ids、JSON keys、reason codes、SSH shard 协议与公开 Rust 数据类型契约。新增内部 console event code 若需要，必须与机器事件协议分开。
- 文件日志中的原有自由文本不做语言迁移；如未来要求文件日志本身随语言变化，属于单独兼容决策，不能混进 human console 的实现。
- 保留 `RUST_LOG`、`LLMUSAGE_LOG`、`LLMUSAGE_PROGRESS`、`NO_COLOR`、`LLMUSAGE_NO_COLOR` 行为；不改默认 file warn。
- 顶层 help、lang、clean 在缺失 home 时不可为日志或语言读取创建运行数据。lang 只有显式设置才写偏好。
- 不重新计算 token，不更改 parsing/dedupe/repair/coverage，不新增 schema/accounting/wire version，不在本轮优化同步算法或承诺提速。
- TUI 已按用户追加需求纳入同一语言设置；Web/desktop/browser dashboard 仍不改，不联网查翻译、不新增第三语言、不发布、不安装、不提交。

## 6. 已确认任务组织

用户已选择完整 CLI，并追加 TUI。已建立 F 语言基础、S 日志/sync、C 其余CLI、T TUI、A 文档/集成验收五子任务，默认 F→S→C→T→A 串行。父任务只整合需求与最终证据，parent/child不是依赖调度器，各 child 文件/meta 明确前置。

本轮已编写父子 prd/design/implement，scope 不再待选；用户的「先不要实现」仍有效，不启动或派发产品实现。最终技术取舍见design，测试机制见validation-plan。功能状态全部NOT_RUN。
