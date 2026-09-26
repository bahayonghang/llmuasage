---
skill: trellis-plan-review
version: 0.5.0
task_dir: "D:/Documents/Code/CLI/llmusage/.trellis/tasks/09-04-llmusage-desktop-mvp"
task_name: 09-04-llmusage-desktop-mvp
task_status: planning
review_scope: task-tree
task_count: 6
task_members:
  - 09-04-llmusage-desktop-mvp
  - 09-04-desktop-shell-ipc
  - 09-04-desktop-core-ui
  - 09-04-desktop-secondary-ui
  - 09-04-desktop-ops-quota
  - 09-04-desktop-windows-bundle
task_statuses:
  09-04-llmusage-desktop-mvp: planning
  09-04-desktop-shell-ipc: planning
  09-04-desktop-core-ui: planning
  09-04-desktop-secondary-ui: planning
  09-04-desktop-ops-quota: planning
  09-04-desktop-windows-bundle: planning
verdict: 需返回规划
blocking: 5
should_fix: 5
notes: 1
generated_at: "2026-09-04T17:11:01.7778178+08:00"
---

# Trellis 任务树规划审阅：09-04-llmusage-desktop-mvp

## 结论

**需返回规划。** 本轮审阅覆盖父任务及 5 个递归子任务，共发现 **5 个阻断项、5 个应修项、1 个提示项**。规划对现有 Web 数据块、超时常量、CSV 导出、配额缓存和 CI 门禁的多数事实陈述是准确的，但还没有把“可复用的 Rust 查询 API”收敛为可执行的 Tauri IPC 输入契约，也没有定义真实取消、运行状态可视化、同步筛选映射和父任务最终集成门禁。因此当前任务树不能安全进入实施。

本轮所有任务均处于 `planning`，故按技能契约跳过实现差异审阅（Pass 7）；报告只评价规划完整性、仓库事实、任务树一致性和可验证性，不评价尚不存在的桌面实现。

## 审阅范围

- 父任务：`.trellis/tasks/09-04-llmusage-desktop-mvp`
- 子任务：`.trellis/tasks/09-04-desktop-shell-ipc`
- 子任务：`.trellis/tasks/09-04-desktop-core-ui`
- 子任务：`.trellis/tasks/09-04-desktop-secondary-ui`
- 子任务：`.trellis/tasks/09-04-desktop-ops-quota`
- 子任务：`.trellis/tasks/09-04-desktop-windows-bundle`
- 审阅对象：每个任务的 `prd.md`、`design.md`、`implement.md`、`task.json`、`implement.jsonl`、`check.jsonl`，以及相关 `.trellis/spec/`、根级 `DESIGN.md` 与被规划引用的 Rust/Web/CI/文档源文件。
- 预检：`plan_precheck.py <parent> --include-descendants` 返回退出码 0，阻断计数 0；任务树结构、必需文件、子任务声明和 JSONL 语法均通过预检。

## 发现摘要

| 编号 | 严重度 | 主题 | 受影响任务 |
|---|---|---|---|
| TPR-01 | 阻断 | IPC 请求 DTO、转换与启动适配契约不可直接实施 | 父任务、shell、core、secondary、ops |
| TPR-02 | 阻断 | “取消上一代请求”缺少后端取消与阻塞任务收束机制 | 父任务、shell、core、secondary |
| TPR-03 | 阻断 | 运行状态、诊断、根目录与锁丢失没有前端责任归属 | 父任务、shell、core |
| TPR-04 | 阻断 | R8.9 同步来源/时间范围映射没有子任务机制或验收 | 父任务、shell、core |
| TPR-05 | 阻断 | 子任务依赖与父任务最终集成验收未闭环 | 全部任务 |
| TPR-06 | 应修 | 设计与实施文档缺少文件级变更表、接口签名和逐步验证 | 全部任务 |
| TPR-07 | 应修 | 验收条件不可追踪且遗漏若干用户可观察行为 | 全部任务 |
| TPR-08 | 应修 | `home_overview` 在核心与二级加载之间归属冲突 | 父任务、core、secondary |
| TPR-09 | 应修 | 视觉来源互相矛盾，部分视觉验收不可判定 | 父任务、core |
| TPR-10 | 应修 | R17 的 macOS/Linux 编译要求没有机制或验证入口 | 父任务、windows |
| TPR-11 | 提示 | Windows JSONL 清单包含即将修改的目标文档 | windows |

## 详细发现

### TPR-01 — IPC 请求 DTO、转换与启动适配契约不可直接实施

- **严重度**：阻断
- **受影响任务**：`09-04-llmusage-desktop-mvp`、`09-04-desktop-shell-ipc`、`09-04-desktop-core-ui`、`09-04-desktop-secondary-ui`、`09-04-desktop-ops-quota`
- **位置**：父任务 `prd.md:23,37-43,52-58,84-85`，父任务 `design.md:23-40,52,60-68`；shell `prd.md:11-16`、`design.md:13-17`
- **规划主张**：桌面命令直接复用根 crate 导出的查询类型，前端请求字段“与 crate 的 serde 字段一致”；启动阶段只通过根 façade、`subscription` 和 `repair_legacy_token_accounting` 访问业务层。
- **仓库证据**：
  - `src/query/filter.rs:8-20` 的 `ReportTimezone` 与 `src/query/filter.rs:28-43` 的 `QueryFilter` 只派生 `Debug/Clone` 等能力，没有 `Deserialize`。
  - `src/query/explorer.rs:14-105,140-157` 的 Explorer 枚举最多为仅输出序列化，`ExplorerFilters`/`ExplorerQuery` 未派生 `Deserialize`。
  - `src/query/logs.rs:23-38` 的 `LogsQuery` 与 `src/query/top_sessions.rs:42-47` 的 `TopSessionsQuery` 未派生 `Deserialize`。
  - `src/commands/serve.rs:206-209` 表明 `repair_legacy_token_accounting` 需要 `&AppContext` 与 `&Store`；`AppContext` 通过 `llmusage::app::AppContext` 可达（`src/lib.rs:41-43`、`src/runtime/app.rs:9-30`），但不在规划列出的根 façade 类型集合中。
  - 配额生产上下文需要独立的 `user_home`（`src/subscription/mod.rs:40-55`），规划只描述“建立生产 FetchContext”，没有说明它与 `AppPaths` 根目录的边界及测试注入方式。
  - 六份设计文档均未给出桌面侧请求 DTO、从字符串/IANA 时区到领域类型的受检转换、数值边界、错误映射或上述启动参数的精确签名。
- **影响**：按当前文档实现时，Tauri 无法直接把前端 JSON 反序列化成多类查询输入；实施者必须临场决定是污染根 crate 的 serde 边界，还是新增桌面 DTO。时区、分页、日期范围和 Explorer 枚举的非法输入也没有唯一错误语义。启动修复与配额上下文同样需要未记录的例外，A2/R9 的“唯一允许依赖面”不可验证。
- **修订路线**：在 shell 设计中逐条写出命令签名与桌面请求 DTO；为 `QueryFilter`、`ExplorerQuery`、`LogsQuery`、`TopSessionsQuery` 等定义显式、可测试的受检转换（含 IANA 解析、页大小/偏移、日期与枚举错误）；明确 `llmusage::app::AppContext` 是否为启动期唯一额外允许类型；明确配额 `user_home` 的生产来源和测试注入。随后让 core/secondary/ops 只引用这些已冻结的 DTO 与错误码，不再自行发明载荷。

### TPR-02 — “取消上一代请求”缺少后端取消与阻塞任务收束机制

- **严重度**：阻断
- **受影响任务**：`09-04-llmusage-desktop-mvp`、`09-04-desktop-shell-ipc`、`09-04-desktop-core-ui`、`09-04-desktop-secondary-ui`
- **位置**：父任务 `prd.md:33,38,89`、`design.md:30,46-49`；shell `prd.md:14,27`、`design.md:9`；core `prd.md:15,26`、`design.md:10`；secondary `implement.md:7`
- **规划主张**：筛选变化时“取消上一代请求”，核心/二级请求分别遵守 3 秒/6 秒截止时间，并以代际编号防止旧结果回写。
- **仓库证据**：
  - 现有 Web 前端不仅有代际保护，还在 `src/web/assets/app.js:421-443` 使用 `AbortController` 终止旧请求。
  - 后端在 `src/web/mod.rs:1657-1734` 同时维护取消原子量、发布 SQLite `InterruptHandle`，并在超时后中断、监督 `spawn_blocking` 查询收束。
  - 当前规划只出现“generation/cancel old invoke/旧代际不回写”描述；命令表、设计和实施步骤中没有取消标识、取消命令、SQLite interrupt、阻塞任务 supervisor 或“超时后后台任务仍继续”的资源语义。
- **影响**：代际检查只能抑制旧结果绘制，不能取消已经进入 SQLite/`spawn_blocking` 的查询。快速切换筛选时，旧查询仍可能堆积并占用连接或 CPU；3 秒/6 秒也只能成为 UI 等待上限，不能证明底层工作已停止。规划因此不能满足其自身的取消主张，也不能复刻现有 Web 的截止时间保护。
- **修订路线**：由产品所有者先在以下两种语义中明确选择一种并写回 PRD/设计/AC：
  1. 保留“真实取消”，则在 shell 定义请求标识、后端取消/超时路径、SQLite interrupt 与阻塞任务收束，并在 core/secondary 验证快速连切后旧工作被终止；
  2. 降级为“仅抑制陈旧响应”，则删除“取消请求/与 Web 等价取消”的主张，明确旧后台工作会继续，并增加并发上限与资源验收。

### TPR-03 — 运行状态、诊断、根目录与锁丢失没有前端责任归属

- **严重度**：阻断
- **受影响任务**：`09-04-llmusage-desktop-mvp`、`09-04-desktop-shell-ipc`、`09-04-desktop-core-ui`
- **位置**：父任务 `prd.md:29-31,44,46,88-90,105-106`；shell `prd.md:15,29-31`、`design.md:8,18-20`；core `prd.md:13-14,27-28`、`design.md:8`
- **规划主张**：桌面完整承接包括“运行状态”在内的十块导航；`LockLost` 等稳定错误必须可见；侧栏显示数据库根目录和锁持有者；核心界面提供相应加载与降级。
- **仓库证据**：
  - `src/query/snapshot.rs:98-110` 的 `DashboardInteractiveSnapshot` 已包含 `health` 与 `diagnostics`，数据源并非不存在。
  - core 设计列出的渲染集合只有 hero、筛选、overview、trends、models、sources、hosts、projects、costs、sync，未包含运行状态/诊断，也没有侧栏根目录或锁持有者组件。
  - shell 只规划最小 `runtime_info` 占位和错误码可测试；其 PRD 把完整 shell 交给 core，但 core 又未接收这些可见责任。
  - 对全部子任务检索 `运行状态`、`diagnostics`、`root_dir`、`lock_lost`、`LockLost`、`锁持有者`，只能命中 shell 的错误/缓存描述，没有前端消费者、状态流或交互验收。
- **影响**：shell 的命令级错误测试可以通过，core 的九类内容渲染也可以通过，但父任务 A5、R10、R12 仍可能缺失。尤其锁在同步期间丢失时，用户可能看不到风险状态，完整可见功能同构无法成立。
- **修订路线**：把“运行状态/诊断 + `root_dir`/锁持有者 + `LockLost` 告警”的组件、数据来源、状态转换、错误显示与 AC 明确归属到一个现有 UI 子任务（建议 core；若归 ops，则更新依赖顺序）；shell 只负责稳定载荷和错误码。补充空闲、运行、失败、锁忙、锁丢失和诊断可用/不可用的可观察验收。

### TPR-04 — R8.9 同步来源/时间范围映射没有子任务机制或验收

- **严重度**：阻断
- **受影响任务**：`09-04-llmusage-desktop-mvp`、`09-04-desktop-shell-ipc`、`09-04-desktop-core-ui`
- **位置**：父任务 `prd.md:39,90,105-106`；shell/core 的 `prd.md`、`design.md`、`implement.md` 全文
- **规划主张**：桌面必须保持同步选项语义：来源、最近 1/7/30 天、全部时间、自定义范围及 `rebuild=false`，并能启动/轮询/取消同步。
- **仓库证据**：
  - 现有 Web 映射在 `src/web/assets/app.js:1628-1637` 明确将选项转换为 `recent_days`、`since`、`until` 和 `rebuild`。
  - 父任务 A7 只断言同步可启动、轮询和取消，没有断言选项映射。
  - 对五个子任务检索 `R8.9`、`recent_days`、`rebuild`、最近 1/7/30 天及自定义范围，没有得到实现机制或验收命中；child map 也没有把该条款分配给任何子任务。
- **影响**：同步按钮与任务轮询即使可用，也可能忽略来源/时间范围、错误地使用 `rebuild=true` 或把“全部时间”映射为默认窗口，造成用户可见的数据变更与父需求不一致。
- **修订路线**：将控件到请求载荷的转换明确归 core，将载荷校验和 `SyncRequest` 构造归 shell；冻结 1/7/30/全部/自定义/来源/`rebuild=false` 的映射表，并为每个分支增加命令层测试和 UI 级可观察 AC。

### TPR-05 — 子任务依赖与父任务最终集成验收未闭环

- **严重度**：阻断
- **受影响任务**：父任务及全部 5 个子任务
- **位置**：父任务 `prd.md:101-111`、`implement.md:3,30-45,62-64`；windows `prd.md:3,11,24-26`、`implement.md:3,11-16`
- **规划主张**：五个子任务按 shell → core → secondary/ops → windows 的顺序形成一个完整、可交付的桌面 MVP，并由父任务统一集成。
- **仓库证据**：
  - 父 PRD 声明 Windows 打包最后执行，但父 implement 只给出“启动第一个子任务”和每个子任务各自验证，没有“所有子任务完成后回到父任务逐条验证 A1-A14”的步骤。
  - Windows 子任务只声明依赖 shell，并写成“核心 UI 可点击后”即可开始；没有依赖 secondary 与 ops 完成。
  - Windows 打包步骤只验证 `tauri build`、NSIS、CI/doc 构建，未运行整棵树的功能、错误状态、持久化、导出、配额和隐私验收。
- **影响**：任务编排允许在 secondary/ops 尚未完成时生成“最终”安装包，也允许五个子任务分别通过局部检查后直接结束，而没有任何关口证明父任务 A1-A14 在同一最终构建中同时成立。
- **修订路线**：把 Windows bundle 的实施前提改为 shell、core、secondary、ops 的最终状态均已集成；在父 `implement.md` 末尾增加唯一的树级集成门禁，使用最终安装/开发构建逐条执行 A1-A14（自动化与人工项分列），记录未验证项，并且只有该门禁通过后才允许任务树进入完成/归档。

### TPR-06 — 设计与实施文档缺少文件级变更表、接口签名和逐步验证

- **严重度**：应修
- **受影响任务**：父任务及全部 5 个子任务
- **位置**：六个任务的 `design.md` 与 `implement.md` 全文
- **规划主张**：父 implement 将五个子任务标记为“设计已经完成，可直接启动”。
- **仓库证据**：
  - 对六份设计检索 `Change list`、`Contract`、`Verification boundary`、`已考虑不做` 等结构，只命中父设计的 Compatibility/Rollback；子设计多为 14–25 行的概要。
  - 没有任务给出将新增/修改的精确文件清单、关键函数或命令签名、错误/数据决策顺序、明确不采用的方案及理由。
  - 六份 implement 均以粗粒度 checklist 加单一总体验证结束，没有在每个实施步骤旁给出文件位置和最小验证命令。
- **影响**：实现者仍需自行选择目录、模块边界、命令 DTO 和测试位置；并行子任务很容易在 `desktop/`、共享状态或前端 stores 上发生责任重叠。后续 Pass 7 也无法稳定比较“规划文件”与“实际改动文件”。
- **修订路线**：为每个子任务补齐最小文件级 change list、关键接口/载荷签名、兼容性与验证边界、至少一个被否决方案；把 implement 拆成可提交的步骤，并为每一步写明目标文件、关键符号和最小验证。保持现有五子任务结构，不为这些文档字段新增抽象层或配置面。

### TPR-07 — 验收条件不可追踪且遗漏若干用户可观察行为

- **严重度**：应修
- **受影响任务**：父任务及全部 5 个子任务
- **位置**：父任务 `prd.md:82-97`；shell `prd.md:24-32`；core `prd.md:24-29`；secondary `prd.md:23-27`；ops `prd.md:23-28`；windows `prd.md:23-27`
- **规划主张**：父子验收条件共同覆盖 R1-R18 及每个子任务的用户故事。
- **仓库证据**：
  - 预检输出只识别到父任务少量 requirement 标识，`criteria=[]`；父任务使用 `A1-A14` 而非可关联的 `ACn [R…]`，子任务大多使用匿名复选项，无法机器或人工稳定追踪到需求条款。
  - 除已在 TPR-03/04/10 单列的缺口外，父/子 AC 仍未明确验证：Explorer 全部输入与禁用原因、热力图单元格钻取、日志每页 20/游标/原始内容、系统保存对话框、自动刷新开关与间隔即时生效、项目/主机下钻、配额命中来源与诊断、R5 serve 兼容和 R6 非上传边界。
  - core 的“hosts 无数据时不喧宾夺主”和 shell 的“不破坏既有命令”等表述没有固定 fixture、视口或可判定阈值。
- **影响**：规划可能在主要页面“看起来可用”时通过验收，但关键筛选、分页、钻取、刷新和隐私条款仍未被执行；修订或交接时也无法知道某项 AC 对应哪条需求。
- **修订路线**：统一采用唯一 AC 标识并在每条后标注所覆盖的 requirement；把复合条款拆成原子、可观察断言。为上述缺失行为补充固定 fixture、操作、期望载荷/状态/文件结果；为视觉项给出视口和可判定条件。不要把已经由 TPR-03/04/10 单列的条款重复成另一套门禁。

### TPR-08 — `home_overview` 在核心与二级加载之间归属冲突

- **严重度**：应修
- **受影响任务**：`09-04-llmusage-desktop-mvp`、`09-04-desktop-core-ui`、`09-04-desktop-secondary-ui`
- **位置**：父任务 `design.md:46-49`、`implement.md:15`；core `prd.md:14,26`、`design.md:9`、`implement.md:9`
- **规划主张**：核心快照应先绘制，`home_overview` 等二级请求随后独立加载；同时 core 子任务又把六张卡片和 `home_overview` 直接调用纳入核心加载/验收。
- **仓库证据**：
  - 现有 Web 在 `src/web/assets/app.js:441-459` 先处理核心结果，在 `src/web/assets/app.js:479-484` 再启动 secondary。
  - 父设计明确把 `home_overview` 放在核心绘制后的 secondary 阶段，父 implement 也允许六张卡片在 secondary 中加载。
  - core PRD/设计/implement 则把 `dashboard_interactive` 与 `home_overview` 并列为核心职责，没有说明后者失败或超时时是否阻塞首次可用绘制。
- **影响**：实施者可能等待两个请求一起完成才渲染，破坏 A6 的 core-first 时序；也可能 core 与 secondary 重复请求同一数据，造成卡片覆盖、闪烁或错误归属不一致。
- **修订路线**：只保留一个所有者。若保持父设计，core 在 `dashboard_interactive` 成功后立即绘制，再以独立 secondary 请求更新六卡，失败不阻塞核心；若坚持 core 发起，也必须明确它是核心绘制后的非阻塞请求，并从 secondary 清单中删除重复责任。同步更新父设计、core PRD/设计/implement 与相关 AC。

### TPR-09 — 视觉来源互相矛盾，部分视觉验收不可判定

- **严重度**：应修
- **受影响任务**：`09-04-llmusage-desktop-mvp`、`09-04-desktop-core-ui`
- **位置**：父任务 `prd.md:26,87`；core `prd.md:7,15,27`；根 `DESIGN.md:5-12,148,204,296,309,335,386-404`；`src/web/assets/base.css:3,22,62`
- **规划主张**：桌面遵循根 `DESIGN.md` 和现有 serve 版 token；空数据 hosts 区域“不喧宾夺主”。
- **仓库证据**：
  - `DESIGN.md` 前部仍定义暖纸张/陶土色体系；第 7 节又标为 Draft，并声明迁移到 Catppuccin、取代旧暖色。
  - 同一第 7 节还把现有界面描述为无新前端构建链的 vanilla Rust-generated HTML，而当前规划明确新增 React/Vite/Tauri 前端。
  - 实际 `src/web/assets/base.css` 使用中性灰与蓝色强调色，既不完全等于前部暖色，也不等于第 7 节 Catppuccin 映射。
  - 规划没有声明三者中哪一份在本任务中具有优先级，也没有为空状态、亮/暗主题、语言切换或响应式视口给出可判定截图/尺寸条件。
- **影响**：不同实施者可以各自合理地选择三套视觉基线并声称满足 R7；“不喧宾夺主”无法客观验收，视觉返工会延后到集成阶段。
- **修订路线**：由产品所有者指定本任务唯一视觉来源及适用章节（实际 serve token、暖色规范或 Catppuccin 草案三选一），明确不适用于 React 桌面的旧实现约束；补充至少一组亮/暗主题、中文/英文、宽/窄视口和空 hosts fixture 的可比较基线及布局判据。

### TPR-10 — R17 的 macOS/Linux 编译要求没有机制或验证入口

- **严重度**：应修
- **受影响任务**：`09-04-llmusage-desktop-mvp`、`09-04-desktop-windows-bundle`
- **位置**：父任务 `prd.md:64,96`；windows `prd.md:11,18-20,23-27`、`design.md:18-20`、`implement.md:11-16`
- **规划主张**：桌面代码在 macOS/Linux 上仍可编译，但本阶段只发布 Windows NSIS。
- **仓库证据**：
  - Windows 子任务继承 R16-R18，却只给出 Windows 端 `npm run tauri build`、安装包和 `just ci` 验证。
  - 全部规划中没有 macOS/Linux runner、`cargo check`/前端构建矩阵、条件编译清单或人工验证边界；“不产出 macOS/Linux release assets”不能替代“可编译”的证据。
- **影响**：R17 可以在没有任何证据的情况下被视为完成，Windows 专用 API、插件或路径假设可能直到后续平台工作才暴露。
- **修订路线**：由产品所有者明确二选一：若 R17 是本 MVP 门禁，补充至少 macOS/Linux 的编译 CI/验证命令和平台条件编译边界；若本阶段无相应环境与证据，则从当前完成条件移除 R17，并明确标为后续未验证目标，而不是保留为已承诺需求。

### TPR-11 — Windows JSONL 清单包含即将修改的目标文档

- **严重度**：提示
- **受影响任务**：`09-04-desktop-windows-bundle`
- **位置**：windows `implement.jsonl:3`、`check.jsonl:3`、`prd.md:15`、`implement.md:9`
- **规划主张**：JSONL 清单为实施/检查阶段提供稳定的 spec/research 上下文。
- **仓库证据**：两个清单都注入 `docs/dashboard/index.md`，而该文件同时被 PRD/implement 明确列为本子任务要修改的交付物；它不是只读 spec/research 依据。
- **影响**：后续上下文注入可能把实施前的目标文档误当成约束来源，且违反清单只承载规范/研究材料、不承载即将修改文件的规划约定。
- **修订路线**：从两个 JSONL 清单移除 `docs/dashboard/index.md`，把它保留在文件级 change list/实施步骤中；如需文档规范依据，改为注入稳定的文档规范或研究材料。

## 已验证且可信的规划部分

- **任务树与工件完整性**：父任务声明的 5 个子任务均存在，必需 Markdown/JSON/JSONL 文件齐全，预检退出码为 0。
- **crate 复用边界的总体方向成立**：`src/lib.rs:73-90` 确实公开导出 `Dashboard`、查询结果/输入相关类型、`AppPaths`、`Store`、`HolderKind`、`JobRegistry` 等；`subscription` 也是公开模块（`src/lib.rs:32`）。问题在具体 IPC 反序列化/转换契约，而不是“完全没有公共 API”。
- **数据库启动顺序与写锁事实准确**：serve 路径确实依次创建 Store、bootstrap、repair（`src/commands/serve.rs:48-50`）；`Store::bootstrap` 通过 `write_operation(HolderKind::Library)` 进入受锁写操作（`src/store/schema.rs:91-135`、`src/store/lock.rs:154-180`）。没有证据支持把 bootstrap 判为无锁写入。
- **核心数据面事实准确**：`DashboardInteractiveSnapshot` 包含 overview、sync、trends、models、sources、hosts、projects、costs、health、diagnostics（`src/query/snapshot.rs:98-110`）；`home_overview` 的摘要结构存在（`src/query/home_overview.rs:37-47`）。
- **现有 Web 时序/数量常量核对通过**：secondary 列表为 10 项且并发上限为 2（`src/web/assets/load-state.js:1`、`src/web/assets/app.js:63,543`）；核心/行为超时分别为约 2 秒/6 秒（`src/web/assets/load-state.js:2-7`），后端行为 API 超时为 3 秒（`src/web/mod.rs:47-48`）。
- **日志与 CSV 主张准确**：日志服务默认页大小为 50（`src/query/logs.rs:9,352-353`），现有 Web 请求每页 20（`src/web/assets/data/fetch.js:365-370`）；CSV 导出确实包含 BOM 和六类区块（`src/web/assets/csv-export.js:20-28`）。
- **配额缓存事实准确**：`UsageEndpoints`、`FetchContext` 与 `fetch_all` 为公开 API（`src/subscription/mod.rs:20-70`），缓存 TTL 为 300 秒（`src/subscription/cache.rs:6,18`）。
- **CI 门禁命名准确**：`.github/workflows/ci.yml:198` 的 job 名为 `CI gate`，`scripts/check-ci-gate.py:24-26` 同步检查该名称；本轮运行脚本返回 `CI gate contract ok`。
- **范围控制与回滚方向基本合理**：任务明确排除了自动更新、签名证书、macOS/Linux 发布物和无界后台常驻，并以删除独立 `desktop/` 为主要回滚面，符合最小 MVP 方向。

## 仍未验证

- 实际 Tauri 2 命令注册、single-instance 行为、原生保存对话框与能力/CSP 配置：`desktop/` 尚不存在，无法进行代码或运行验证。
- 3 秒/6 秒超时后的阻塞查询是否真正终止、SQLite 连接是否及时释放：当前仅有规划，没有桌面实现。
- Windows NSIS 产物、卸载器、安装/卸载流程、SmartScreen 提示和 GitHub Actions artifact：需要 Windows 构建与人工安装证据。
- macOS/Linux 编译：规划未提供 runner 或命令，保持 `UNVERIFIED`。
- 亮/暗主题、中文/英文、窄窗口、空数据、无障碍和与 serve 版的视觉同构：需要固定 fixture 与实际渲染检查。
- R6 所述“不上传会话内容”、配额真实宿主机读取及诊断来源：只能在桌面 wrapper 落地后通过网络/日志/fixture 边界验证。
- 单实例激活旧窗口、焦点恢复与关闭/重开流程：属于本地 GUI/操作系统行为，当前保持 `UNVERIFIED`。

## 盲点

No blind spots were found given the inspected source files and the fact that every task in scope is still in planning.

## 建议的修订顺序

1. 先由产品所有者裁决 TPR-02、TPR-09、TPR-10 中的语义选择；这些选择会改变 PRD，而不是实施者可自行补齐的细节。
2. 修订 shell 的 IPC DTO/转换、取消、启动上下文与错误契约（TPR-01、TPR-02）。
3. 修订父子责任图：补齐运行状态/锁信息与同步选项映射（TPR-03、TPR-04），统一 `home_overview` 时序（TPR-08）。
4. 补齐每个子任务的文件级设计、接口签名、逐步验证和 requirement→AC 追踪（TPR-06、TPR-07），机械清理 Windows JSONL（TPR-11）。
5. 收紧依赖顺序并新增父任务最终集成门禁（TPR-05），然后重新运行父任务递归预检与本技能审阅；在此之前不要执行 `task.py start`。
