# 优化看板侧栏状态与同步告警

## Goal

让看板左下角成为简洁、稳定、可读的本地服务状态区，并让右上角“数据状态”只反映当前同步健康，而不是把已经恢复的历史命令中断误报成当前同步失败。

## Background

- 桌面侧栏宽度为 248px。`src/web/shell.rs:128` 的主题/语言按钮与 endpoint 卡片目前各自成块，endpoint 的窄列元信息容易换行。
- `src/web/assets/app.js:1709` 会把同步任务的自由文本 `snapshot.summary` 写入 `#endpoint-sync`。实际完成摘要包含 `sources=... seen=... inserted_delta=... stored_events=...`，在侧栏中被强制拆成多行。
- `src/web/assets/render/hero.js:75` 根据 `health.recent_failures` 的数量决定右上角告警。该健康集合来自最近十条所有命令记录，包括已恢复的 `serve` 中断。
- 当前本机诊断中的两条记录都是 `serve / aborted / recovered stale running record`；最新 `sync` 为 `success`，`sync_command_center` 同时返回 `tone=good` 与 `headline_key=syncCenter.headline.ready`。
- `src/query/mod.rs:3480` 已定义同步状态的权威语义：worker lock 忙或最新 usage-import 失败才告警；后续成功同步应恢复为 ready。历史 `aborted` 记录仍保留在诊断详情中。

## Requirements

### R1. 侧栏底部信息层级

- 主题与语言控制保持现有功能、DOM id、键盘焦点和中英文切换能力，但在桌面侧栏中呈现为统一、紧凑的偏好设置控件。
- endpoint 卡片清楚区分在线指示、本地地址与最近同步时间；地址和时间在 248px 侧栏内不得被技术摘要挤压或出现难读的逐词换行。
- `#endpoint-sync` 只承载稳定的最近同步时间或简短占位，不再承载 job summary、job id 或原始错误文本。同步进度、结果和错误继续由顶部按钮、同步命令中心和加载错误面板承载。
- 720px 及以下保持当前横向紧凑导航：endpoint 隐藏，主题/语言控制仍可见且不扩大导航高度。

### R2. 当前同步健康语义

- 右上角“数据状态”的 tone 与摘要必须消费已规范化的 `syncCommandCenter`，不得再以通用 `health.recent_failures` 数量判断当前同步失败。
- `good` 表示普通同步可用且最新 usage-import 未失败；`warn` 表示 worker lock 忙或最新 usage-import 失败；缺少同步状态时显示中性等待态。
- 卡片的两个快速指标展示当前同步维度（就绪来源与最新同步结果），不再把历史所有命令异常数量作为当前数据健康指标。
- 不删除、不改写 `run_log`；历史 `serve` 中断、同步失败和其他诊断记录继续保留在运行状态/诊断详情中。

### R3. 刷新与国际化一致性

- hero 的渲染指纹包含 `sync_command_center`，使后台状态变化、同步完成和语言切换都能刷新右上角状态。
- 新增或调整的用户可见文案同时覆盖中文和英文；技术字段不得直接泄漏到侧栏紧凑区。
- snapshot/旧 payload 缺少 `sync_command_center` 时保持可用，显示中性空态而不是抛出异常。

### R4. 文档与回归覆盖

- Dashboard 文档明确区分“当前数据同步健康”和“历史运行诊断”。
- 增加聚焦回归，固定 hero 的状态来源、render-key 依赖、侧栏不渲染自由文本摘要，以及桌面/移动响应式约束。

## Acceptance Criteria

- [x] 使用当前本机 payload（最新同步成功，同时存在两条已恢复的 `serve` 中断）时，右上角显示正常/就绪，不显示“存在失败”；两条历史记录仍可从诊断数据读取。
- [x] 最新 usage-import 为 `failed` 或 worker lock 为 busy 时，右上角仍显示警告；后续成功同步后恢复正常。
- [x] 左下角只显示本地服务地址和简短同步时间，完成一次同步后不会出现 `sources=...`、`inserted_delta=...` 等摘要换行。
- [x] 主题与语言按钮在桌面侧栏中视觉成组，hover/focus 可辨识；在 720px 及以下保持紧凑且 endpoint 隐藏。
- [x] 中文、英文、空 payload 和窄屏状态均不会出现缺失文案、脚本异常或明显布局溢出。
- [x] 聚焦 Rust/Node 检查、完整 `just ci` 与本地浏览器桌面/窄屏验收通过。

## Out of Scope

- 清理用户数据库、删除历史运行记录或改变 `RunRecord::counts_as_failure` 的通用 doctor/diagnostics 语义。
- 修改同步导入算法、worker lock、API payload schema 或 SQLite migration。
- 重做侧栏导航、主看板布局、同步命令中心或事件日志页面。

## Constraints

- 不新增生产依赖。
- 复用现有 `sync_command_center` 权威投影与现有主题/i18n 机制。
- 保留 public read-only dashboard 的敏感数据边界；本任务不扩展公开 payload。
