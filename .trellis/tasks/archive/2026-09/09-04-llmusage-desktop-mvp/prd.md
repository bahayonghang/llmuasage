# llmusage Desktop MVP

## 审阅修订（2026-09-04）

产品所有者确认：

- TPR-02：真实取消（1A）。筛选切换会中断上一代 SQLite 查询并收束阻塞任务。
- TPR-09：唯一视觉来源为 live serve token（2A），文件 `src/web/assets/base.css`。
- TPR-10：macOS/Linux 可编译移出本任务完成条件（3A），列为后续未验证目标。

## Goal

交付独立的 llmusage Desktop：原生窗口进程内调用本仓库 crate，读写同一份 `~/.llmusage/llmusage.db`。第一版对齐 live `llmusage serve` 的全部可见面板与同步生命周期，加上 TUI Usage 的订阅额度，并补上离开浏览器后应用无法自立的缺口。用户不必再开 `llmusage serve`。Windows 上可本地打出未签名 NSIS/exe。

## Background

- 独立产品，不扩 ccr-ui。路径 B：进程内 crate + 新前端 + Tauri IPC。
- 面板范围 C：对齐 live serve，不是核心快照子集。
- 额度进 MVP：复用 `subscription::fetch_all`，不新写拉取器。
- 前端：Tauri 2 + React 19 + Vite。不把 `src/web/assets/` 当产品 UI。
- 对照清单：`research/serve-parity-and-desktop-gaps.md`。
- 嵌入面：`docs/reference/library-api.md`。写入：`.trellis/spec/llmusage/backend/write-fencing-contracts.md`。live 加载：`.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`。额度：`.trellis/spec/llmusage/backend/tui-subscription-contracts.md`。CI：`.trellis/spec/llmusage/backend/ci-toolchain-contracts.md`。

## Requirements

### 产品边界

- R1: 独立原生窗口，产品名 llmusage Desktop。代码在本仓库 `desktop/`。
- R2: 进程内 path 依赖根 crate。前端只经 Tauri command。不启动 axum，不打包 `llmusage` sidecar。根 `Cargo.toml` 不改成 workspace（Desktop 自有 `desktop/src-tauri/Cargo.toml`）。
- R3: 默认 `AppPaths::discover()` 打开同一份库。不另建 Desktop SQLite。
- R4: 查询走公开 `Dashboard` 方法。输入由 **Desktop 请求 DTO** 经受检转换得到 `QueryFilter` / `ExplorerQuery` / `LogsQuery` / `TopSessionsQuery` / `SyncOptions`。禁止给这些根 crate 查询输入类型加 `Deserialize`。额度走 `llmusage::subscription`。启动 repair 走已公开的 `llmusage::commands::serve::repair_legacy_token_accounting`。启动期唯一额外允许类型：`llmusage::app::AppContext`（只为调用该 repair）。允许的根 crate 窄补丁：把 `Dashboard::interrupt_handle` 从 `pub(crate)` 改为 `pub`。Desktop crate 不写 SQL，不重做 parser。
- R5: 不修改 `llmusage serve` 的监听、公开面、浏览器打开契约。
- R6: 不上传 session 内容。行为事实不存完整 prompt。不把 `codex-tracer.db` 并进主库。桌面进程对外网络只用于额度拉取（测试注入本地监听）。
- R7: 本任务唯一视觉来源：live serve 的 `src/web/assets/base.css`（含 `[data-theme='light'|'dark']` 语义 token）。布局节奏对齐 `src/web/assets/layout.css`（侧栏 248px，≤720px 折叠）。`DESIGN.md` 第 1–2 节暖纸张/陶土色与第 7 节 Catppuccin 草案不作为本任务验收。第 7 节「vanilla HTML、无新前端构建链」不适用于本 React/Tauri 前端。

### Serve 面板对等

- R8: 十个导航块：用量概览、用量趋势、模型分布、来源分布（hosts 行数 > 1 时展示）、项目排行、行为分析、用量分析、成本估算、运行状态、事件日志。
- R8.1 概览：hero、六张 summary 卡、筛选轨（source / model / range 1d·7d·30d·all·custom / since / until）、Daily activity、Weekly activity、Session ranking、Token usage mix、sync command center。六张卡由 `home_overview` 在核心绘制之后独立加载。
- R8.2 趋势：window `day`/`week`/`month`/`all` 与 range 映射：`1d→day`、`7d→week`、`30d→month`、`all→all`；custom 沿用当前 window，缺省 `day`。
- R8.3: 行为：Activity、Tool usage、Optimize（只读）、Model compare；展示 `normalized` / `no_data` / `degraded` / `unsupported` / `insufficient_models` / `low_sample`。这四块超时 3s（对齐 `WEB_BEHAVIOR_API_TIMEOUT`），超时返回 degraded payload，不把错误画成 0。
- R8.4 Explorer：metric、group_by、granularity、limit、session_id、tool_name、tool_kind、token_type、include_other、include_non_tool。`include_non_tool=false` 映射为 `ExplorerFilters.is_tool = Some(true)`。
- R8.5: Logs：每页 20 行游标分页（对齐 live `LOGS_PAGE_SIZE`，command 显式传 `page_size=20`，不使用 `LogsQuery` 在 page_size=0 时的默认 50）。从 session ranking 带 session 过滤跳入；展开行按需读 raw。
- R8.6 顶栏：同步/取消、自动刷新 off/30s/60s、导出当前筛选 CSV、主题、zh/en。自动刷新间隔变更后立即按新间隔重新计时。
- R8.7 筛选对齐 `QueryFilter`，含 timezone（默认本机 IANA）以及从项目/host 钻取的 `project_hash` / `host_id`。非法 IANA/日期/来源/枚举返回 `invalid_request`。
- R8.8 加载对齐 live 看板：先 `dashboard_interactive`，再次级并发 2；**真实取消**（代际 + `cancel_queries` + SQLite `InterruptHandle` + 阻塞任务 supervisor）；核心失败不扇出旧 section 组合；次级降级不挡核心。读连接使用 `Dashboard::open_with_busy_timeout`（1500ms）。核心 UI：2s 标慢、6s 失败并取消该请求。
- R8.9 sync 选项对齐 serve。控件到 `SyncStartDto` 的转换归 core；`SyncStartDto` → `SyncOptions` / `ValidatedSyncRequest` 归 shell。映射表：

  | UI range | `recent_days` | `since`/`until` 进入 sync 载荷 |
  |---|---|---|
  | 1d | 1 | 不传 |
  | 7d | 7 | 不传 |
  | 30d | 30 | 不传 |
  | all | 不传 | 不传 |
  | custom | 不传 | 不传（custom 的 since/until 只进入查询 FilterDto） |

  当前 source filter → `source`。shell 构造时 `rebuild` 恒为 `false`。界面不提供 `--rebuild` / `--allow-lossy-rebuild`。`recent_days` 越界（非 1..=3650）→ `invalid_request`。

### 独立 Desktop 自立

- R9: 库不存在或 `NotInitialized` 时进程内 `Store::bootstrap`，然后调用与 `llmusage serve` 相同的 `repair_legacy_token_accounting(&AppContext, &Store)`。不要求先跑 CLI `init`。
- R10: `SchemaTooNew`、`LockBusy`、`LockLost` 可见，不损坏库。sync 走 `JobRegistry` + `HolderKind::Library`。
- R11: 单实例：第二进程聚焦已有窗口后退出。
- R12: 侧栏展示 `root_dir` 与锁持有者摘要，不展示 HTTP 地址。运行状态块消费 `dashboard_interactive.health` / `diagnostics` 与 `runtime_info`。
- R13: CSV 经系统保存对话框写出。公式中和与 `src/web/assets/csv-export.js` 一致：当前筛选下的 summary、daily、projects、models、sources、sessions，UTF-8 BOM。
- R14: 主题、语言、自动刷新、上次筛选写入 `AppPaths.root_dir.join("desktop.json")`，不依赖 URL。

### 订阅额度

- R15: 独立导航块，对齐 TUI Usage：
  - `llmusage::subscription::fetch_all`；进入可用 5 分钟缓存；显式刷新 `bypass_cache=true`。
  - 生产 `FetchContext.user_home` = `llmusage::util::resolve_home_dir()`（凭证目录，与 `AppPaths.root_dir` 分离）。生产 `cache_path` = `AppPaths::subscription_cache_path()`。
  - 测试注入 `user_home`、`cache_path`、`UsageEndpoints` 到临时目录与本地监听。
  - 无本地凭证的提供者跳过。
  - 凭证只读，不刷新 token，不改凭证文件。
  - 邮箱默认 `[hidden email]`，可切换。
  - 展示 `UsageFetchReport.outputs` 与 `diagnostics`，以及 Desktop 包装字段 `cache_hit`。
  - CI 禁止打公网额度主机。
  - 额度失败不阻断 R8。

### 交付与门禁

- R16: Windows 上 `tauri build` 打出未签名 NSIS 或 exe。不接代码签名、GitHub Release、updater。
- R18: 根 `just ci` 与 GitHub `CI gate` 作业名不变。Desktop 自有 `just` recipe（dev / test / build）。不把 `tauri build` 塞进根 `just ci`。

### 后续未验证（TPR-10 / 3A）

macOS/Linux 可编译曾写入完成条件。本任务完成条件不包含该证据，不提供跨平台 runner 或 `cargo check` 矩阵。不产出 macOS/Linux 发版资产。后续任务需要时再补验证。

## Out of scope

- sidecar / 进程内 axum 包现有 serve 页
- 并入 ccr-ui
- 新 parser / 新 source
- 改 `llmusage.db` schema
- 给根 crate 的 `QueryFilter` / `ExplorerQuery` / `LogsQuery` / `TopSessionsQuery` 加 `Deserialize`
- 云同步、账号、遥测
- `--public`、SSH 隧道 UI、HTTP CSRF 面
- `export html` snapshot 浏览模式
- `codex-tracer`、`llmusage remote` 管理 UI、`catalog apply` / 联网刷价 UI
- vanilla 看板没有的 `diagnostics/forget` 按钮
- 托盘、开机自启、系统通知
- 签名安装包、自动更新
- 新额度提供者；把 token 写入缓存
- macOS/Linux 编译验收与发版资产
- 按 `DESIGN.md` 暖色或 Catppuccin 草案验收视觉

## Acceptance Criteria

- [ ] AC1（R1, R3）：不运行 `llmusage serve` 也能打开 Desktop 并读到本地库用量。
- [ ] AC2（R4, R9）：command 只调用根 façade、`llmusage::subscription`、启动路径上的 `llmusage::app::AppContext` + `repair_legacy_token_accounting`，以及 Desktop DTO 转换；测试用 `features = ["testing"]` 的 `Fixture`，不碰真实 `~/.llmusage`。
- [ ] AC3（R10）：与 CLI `sync` 互斥：一端持锁时另一端得到忙/失败展示，库不被双写损坏。
- [ ] AC4（R18）：根 `just ci` 与 `CI gate` 作业名不被 Desktop 脚手架破坏。
- [ ] AC5（R8）：R8 十个导航块可打开，并渲染对应 façade payload（空/降级不装成 0）。
- [ ] AC6（R8）：核心快照可在次级仍 loading 或 degraded 时显示。
- [ ] AC7（R8, R10）：同步可启动、轮询、取消；`LockBusy` 时按钮与 sync center 说明被占用。1d/7d/30d/all/custom/source 按 R8.9 表进入 `SyncStartDto`；实际 `ValidatedSyncRequest.rebuild()` 为 false。
- [ ] AC8（R9, R10）：未初始化库经 Desktop 首次启动（bootstrap + token-accounting repair）后可查询；`SchemaTooNew` 拒绝写入并提示升级 Desktop。
- [ ] AC9（R11）：第二实例不出现第二个写进程。
- [ ] AC10（R13）：导出 CSV 含 summary、daily、projects、models、sources、sessions，UTF-8 BOM，公式中和；经系统保存对话框写出。
- [ ] AC11（R14）：主题、语言、刷新间隔、筛选在重启后恢复。
- [ ] AC12（R15）：有本地凭证时可看到额度 outputs；无凭证时为空态。刷新走 bypass_cache。邮箱默认隐藏。
- [ ] AC13（R15, R6）：额度测试不接触公网；凭证字节在拉取前后不变。额度失败时 R8 仍可用。
- [ ] AC14（R16）：本机 Windows 上 `just desktop-build`（或文档中的等价命令）产出未签名 NSIS/exe。
- [ ] AC15（R8）：快速连切筛选后，上一代 `request_id` 的 SQLite 查询被 `interrupt`；supervisor 在超时/取消后收束 `JoinHandle`；旧代际结果不回写。允许的观测：测试夹具里超时查询返回 `cancelled`/`timeout`，且 `inflight` 在收束后回到 0。
- [ ] AC16（R10, R12）：侧栏显示 `root_dir` 与锁持有者摘要。运行状态块可区分 idle、running、failed、lock_busy、lock_lost；diagnostics 有数据时展示，无数据时为明确空态。`LockLost` 出现告警，不继续写库。
- [ ] AC17（R8）：Explorer 全部输入可提交；`unsupported` 时对应控件禁用并显示原因，不把结果画成 0。
- [ ] AC18（R8）：点击 heatmap 日期进入该日 custom 范围并重拉；再点同一格还原到点击前的 range。
- [ ] AC19（R8）：logs 每页 20 行；下一页带游标且不重复上一页首行；展开行才请求 raw。
- [ ] AC20（R8）：自动刷新在 off/30s/60s 之间切换后，下一次刷新按新间隔触发，无需重启进程。
- [ ] AC21（R8）：点击项目行把 `project_hash` 写入筛选并重拉；点击 hosts 行（仅当 hosts 行数 > 1）写入 `host_id` 并重拉。
- [ ] AC22（R15）：进入额度页且缓存未过期时 `cache_hit=true`；点刷新后 `cache_hit=false`，并展示本次 `diagnostics`。
- [ ] AC23（R5）：本任务对 `src/commands/serve.rs`、`src/web/mod.rs` 监听/公开面/打开浏览器相关代码的 diff 为空。
- [ ] AC24（R6）：除额度 `FetchContext.endpoints` 外，Desktop 不发起把 session 内容或 raw JSON 送出本机的网络请求。额度测试只打本地监听。
- [ ] AC25（R7）：固定 fixture：亮/暗主题、zh/en、视口 1440×900 与 720×800。`--bg-primary` / `--accent` 与 `base.css` 对应主题一致。hosts 行数 ≤ 1 时 `#hosts` 等价区域不展示（0 高度、不占 sources 主列）。1440px 侧栏宽 248px；720px 侧栏改为顶栏横排，页面无横向溢出。
- [ ] AC26（R8）：`dashboard_interactive` 成功后立即绘制核心；`home_overview` 作为次级列表中的独立请求。该请求失败或超时时六卡为 degraded/空态，核心其余块保持已绘制内容。
- [ ] AC27（R2）：Desktop 以独立 `desktop/src-tauri/Cargo.toml` path 依赖根 crate；根 `Cargo.toml` 未被改成 workspace。

## Child task map

父任务拥有本 PRD。实施从子任务开始，不在父任务里改产品代码。

| 子任务 | 验收切面 |
|---|---|
| desktop-shell-ipc | R1–R4, R6, R9–R12, R8.8 取消/超时, R8.9 载荷校验；AC2, AC3, AC8, AC9, AC15 的 command/状态层 |
| desktop-core-ui | R7, R8 核心块, R8.2, R8.6 主题/语言/同步按钮, R8.7, R8.8 核心时序, R8.9 控件映射, R10/R12 运行状态 UI；AC5 核心, AC6, AC7 UI, AC16, AC21, AC25 |
| desktop-secondary-ui | R8.1 次级（含 `home_overview` 六卡）, R8.3–R8.4；AC17, AC18, AC26 |
| desktop-ops-quota | R8.5–R8.6 的 logs/CSV/刷新, R13–R15, R6 额度网络边界；AC10–AC13, AC19, AC20, AC22, AC24 |
| desktop-windows-bundle | R16, R18；AC4, AC14。gitignore、just、文档。不含 macOS/Linux 编译验收 |

子任务顺序：shell-ipc → core-ui → secondary-ui 与 ops-quota（core 之后可并行）→ windows-bundle（**shell、core、secondary、ops 均完成后**才打最终安装包）。

父任务在五子任务产品改动齐备后执行唯一树级集成门禁（见父 `implement.md`），逐条核对 AC1–AC27。该门禁通过前，任务树不得进入完成/归档。
