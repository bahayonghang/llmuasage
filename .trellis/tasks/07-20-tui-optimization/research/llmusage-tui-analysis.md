# llmusage TUI 深度分析（2026-07-20）

> 由并行分析 agent 产出的原始事实记录，file:line 均指向本仓库当前 dev 分支。
> 结论摘要见父任务 prd.md；本文件是子任务实现时的证据底稿。

## 1. ARCHITECTURE

ENTRY / DISPATCH
- main.rs:1 `#[tokio::main]` (bare = multi-thread runtime) → lib.rs:94 `run()` async → lib.rs:102 `dispatch(app,cli).await`.
- commands/mod.rs:302-303 `Dash`/`Tui` → `dash::run(&app, deprecated).await`.
- dash.rs:31-44 `run()` is async but calls `tui::run_terminal(&store)?` SYNCHRONOUSLY (blocking) — so the entire crossterm blocking loop runs ON a tokio worker thread inside the outer runtime context. (`commands/tui.rs` is a second near-identical entry; both call `tui::run_terminal`.)
- tui/mod.rs:35 `run_dashboard` → :62 `event_loop`. Backwards-compat alias `run_terminal` at :80.

TERMINAL SETUP/TEARDOWN (tui/mod.rs)
- Panic hook installed BEFORE raw mode (:42-52) restores terminal on panic.
- :55-59 enable_raw_mode + EnterAlternateScreen + EnableMouseCapture, CrosstermBackend, Terminal::new.
- :65-71 cleanup always runs after loop (disable raw, DisableMouseCapture, LeaveAlternateScreen, Show).

EVENT LOOP (tui/mod.rs:84-177)
- Dashboard::open once at :85 (single rusqlite Connection, reused for ALL queries for whole session).
- AppState::new at :86; initial resize :88; EventHandler tick=250ms :89.
- Initial Overview load :92 (before first draw).
- Loop: `terminal.draw(...)` at :97 EVERY iteration, then blocks on `events.recv()` at :99. Tick → on_tick + maybe refresh → `continue` (→ redraw). Resize → handle_resize → continue. Mouse → maybe SwitchPanel. Key → dialog handler or handle_key_event.
- REDRAW STRATEGY: unconditional redraw every event; no dirty tracking. Idle cadence = 1 redraw / 250ms tick (~4 fps) forever.

THREADING MODEL
- event.rs:13-55 EventHandler spawns ONE std::thread that `event::poll(250ms)` + `event::read()`, forwards Key(Press/Repeat only, :62-64)/Mouse/Resize over std `mpsc` (UNBOUNDED) channel; on poll timeout or any unmatched event sends Tick.
- Main thread does: all rendering + all DB queries + sync. No worker threads for data.
- SYNC path (see §5) builds a NESTED tokio runtime.

## 2. DATA FLOW

- All queries go through `Dashboard` (query/mod.rs:759-773) holding one `Connection` (store/connection.rs:21-36: WAL, busy_timeout 30s, opened once).
- Filter is ALWAYS `QueryFilter::default()` (app.rs:276) → filter.rs:44-55 has NO since/until → every bucket/event query is a FULL-TABLE scan (only optional `source=?` clause when a source filter is set).
- LAZY LOAD + CACHE: load_panel_data (mod.rs:248-323) loads a panel's data only when its `Option` cache is `None`. Switching away and back does NOT re-query. Caches live in AppState (app.rs:234-246).
- Load happens SYNCHRONOUSLY inside the action handler (SwitchPanel mod.rs:130-133) BEFORE the next draw. Consequence: the None→"Loading…" placeholder branch never actually renders; the UI FREEZES during the query, then draws populated data. "加载中…"/"Loading…" states are effectively dead (only Err/empty are reachable).
- INVALIDATION (app.rs:370-388): sets ALL caches None + resets every scroll offset + needs_refresh=true. Triggered by: `r` refresh (mod.rs:209-215), source filter change (app.rs:351,365), sync completion (mod.rs:197), auto-refresh interval.
- AUTO-REFRESH: default OFF (app.rs:294), interval 30s (app.rs:295). on_tick (app.rs:404-409) sets needs_refresh when elapsed; Tick handler (mod.rs:103-106) calls refresh_panel_data → invalidate + reload active panel. Only the ACTIVE panel reloads; others reload lazily on next visit.

PER-PANEL QUERIES (mod.rs:248-398) — all synchronous on main thread, filter=default(full scan):
- Overview → `overview()` (query/mod.rs:789): ~7 aggregates over usage_bucket_30m (lifetime token summary, 24h summary, 2 event counts, cost sum, distinct-source, bucket count) + 2 run_log MAX.
- Trends/"Usage" → `sync_command_center()` (query/mod.rs:2164): calls `diagnostics()` (source_file state counts + run_log) + sync statuses + recent_runs + worker-lock read.
- Models → `model_breakdown()` (query/mod.rs:932): full GROUP BY model, many SUMs + 3 pricing CASE aggregates.
- Sources/"Daily" → `trends_daily()` (query/mod.rs:894): full GROUP BY local date.
- Projects/"Hourly" → `trends("day")` (query/mod.rs:847): usage_bucket_30m bounded to last 24h (the ONE bounded query). Hardcoded "day"; ignores time_window.
- Cost → `cost_breakdown()` (query/mod.rs:1175): full GROUP BY source,model.
- Health/"Stats" → StatsPanelPayload (mod.rs:379-398): FIVE queries — overview() + heatmap(365) + source_breakdown() + health() + context_pressure().
  - source_breakdown (query/mod.rs:1088): full GROUP BY source, THEN N+1 — one extra `MAX(event_at)` query over usage_event PER source (:1115-1127).
  - context_pressure (query/mod.rs:1000): full GROUP BY source,model over RAW usage_event (heavier than buckets) + per-group catalog lookup.
- Behavior/"Agents" → BehaviorPanelPayload (mod.rs:356-377): FIVE payloads — activity_breakdown + tool_breakdown + optimize + zombie_report(InventoryRoots::discover()) + model_compare(None,None).
  - tool_breakdown (query/mod.rs:1305): large multi-CTE over usage_event+usage_tool_call w/ UNION ALL + DISTINCT counts, LIMIT 50.
  - optimize (query/mod.rs:1442): 4 detector queries, each joins usage_tool_call/usage_turn↔usage_event (full scans).
  - zombie_report (query/mod.rs:1514): FILESYSTEM scan (InventoryRoots::discover().scan()) + 2 DISTINCT queries.
  - model_compare (query/mod.rs:1778): candidates + per-model×2 (token+turn+tool) + category compare×2 ≈ 7 queries.
- Blocks → `blocks_report()` (query/mod.rs:1067 → reports.rs:763 load_blocks_report): STREAMS ALL usage_event rows (visit_filtered_events, filter has since/until=None) building rolling 5h aggregates, THEN discards all but active + last-3-days (reports.rs:800-810). Full event-table scan regardless of the 3-day display window.

## 3. FEATURES INVENTORY

9 PANELS (app.rs:15-25 enum; labels app.rs:76-102). Note enum name ≠ displayed label ≠ actual data:

| idx | enum | label(wide/short) | render module | data shown |
| --- | --- | --- | --- | --- |
| 0 | Overview | Overview / Ovw | panels/overview.rs | KPIs + token mix + activity + freshness + 24h pulse |
| 1 | Trends | Usage / Use | panels/usage.rs | sync command center (summary+source sync table+monitor) |
| 2 | Models | Models / Mod | panels/models.rs | per-model table |
| 3 | Sources | Daily / Day | panels/daily.rs | daily trend table |
| 4 | Projects | Hourly / Hr | panels/hourly.rs | hourly trend table + ASCII bar |
| 5 | Cost | Cost / Cost | panels/cost.rs | per source,model cost table |
| 6 | Health | Stats / Sta | panels/stats.rs | summary + contribution heatmap + source mix + health |
| 7 | Behavior | Agents / Agt | panels/behavior.rs | activity/tools/optimize/zombie/compare (4 stacked sections) |
| 8 | Blocks | Blocks / Blk | panels/blocks.rs | 5h burn-rate blocks table |

(draw dispatch: draw.rs:25-71.)

KEYBINDINGS (input.rs:34-54):
- q / Esc → quit
- Tab → next panel; Shift-Tab(BackTab) → prev panel
- 1–9 → jump to panel (Panel::from_digit_char, 1..=9; '0' ignored)
- j/↓ scroll down, k/↑ scroll up
- l/→ NextWindow, h/← PrevWindow → **NO-OP in shipped UI** (dead code, see §5)
- r refresh (invalidate+reload), R toggle auto-refresh, x start sync, s source picker, ? help, t cycle theme
- Mouse: LEFT-CLICK on nav bar row switches panel (mod.rs:112-118,235-245 → nav_bar::panel_at_position). Wheel/scroll NOT handled though mouse capture is on.

DIALOG KEYS (input.rs:56-65): Esc/q close, j/↓ down, k/↑ up, Enter/Space select, a clear-source. Help dialog honors ONLY close (mod.rs:218-223); other keys ignored.

SOURCE PICKER (source_picker.rs): centered 74×20, Clear, lists all 23 platform probes with marker [*]=active /[ ]=selectable /[-]=monitor-only; cols name/status/parser/quality. Enter toggles filter.source (only probes with source_kind); monitor-only probes just set a status msg (app.rs:354-360). 'a' clears filter. Own scroll math (:58-65).

HELP OVERLAY (help_dialog.rs): centered 72×14, shows theme+source, static keybind list, footer note.

SORT/FILTER/SCROLL: sorting is FIXED by SQL ORDER BY (no user toggle). Filtering only by source (picker). Scrolling = per-panel ScrollState offset (app.rs:177-194), keyboard only, clamped by update_scroll_total (mod.rs:325-350). Long-tail collapse "+N more" row for models/cost/sources (panels/longtail.rs, KEEP_MIN=8, TAIL_SHARE=2%).
TIME RANGE: TimeWindow enum exists (app.rs:126-174, Day24h/Week7d/Month30d/All, default Week7d) but is DEAD (§5).
LIVE REFRESH: manual `r` + optional 30s auto (`R`). Sync via `x`. NO search, NO export/CSV, NO date picker, NO session drill-down, NO column sort. Read-only by design.

## 4. UI STYLE INVENTORY

LAYOUT: draw.rs:16-21 vertical [nav=Length3, content=Min0, footer=Length4].
- nav_bar.rs: bordered block, title " llmusage " left + " local usage " right; tabs " N Label " joined by " │ " (border_normal); active=black-on-accent bold (theme.rs:270-275), inactive=white (:278-280). very_narrow(<60)→short labels.
- footer.rs: bordered block, rows [controls, status, blank]. Controls = keybind hints, DIFFERENT set when is_very_narrow(<60). Status line: "source <label> • " + (status_message green-bold | overview "N tokens • $X" | "local dashboard cache").
- Panels: theme::panel_block (borders ALL, accent bold title) theme.rs:303-308, or per-panel styled_block. Cards: trend_card_block (theme.rs:311-319) / overview KPI cards with per-card color.

THEME (theme.rs): process-wide `RwLock<Theme>` (:129-142). 2 themes: default_dark (:46, Cyan/DarkGray/lazygit-like) + catppuccin_mocha (:76, truecolor). Cycle with `t` (:154-163); LLMUSAGE_THEME env at startup (mod.rs:37-39). Theme is `Copy`; every accessor (accent(), muted_fg(), header_style()…) takes a read-lock and copies the whole struct — called hundreds of times per frame (per Span).
COLOR USAGE: theme has semantic slots (accent, muted_fg, kpi_colors[4], heat[5] ramp, bar_ok/warn/danger, trend_*). BUT many panels HARDCODE ratatui `Color::{Green,Cyan,Yellow,Magenta,Blue,Red}` instead of theme slots — overview.rs (all metric_line colors), daily.rs, hourly.rs, stats.rs, usage.rs, behavior.rs, health.rs. → theme switch only recolors a SUBSET; hardcoded colors stay fixed. report_table.rs (CLI path) has its own crossterm `source_color` (:975), separate palette.

TABLES: ratatui Table, header theme::header_style (bold) + bottom_margin(1); alternating row bg via row_alt_style when index%2==1. Widths: mixed Constraint::Percentage and ::Length per breakpoint. Responsive: each table panel computes very_narrow/narrow from inner.width (thresholds vary 54–92) → different column sets/labels. Several panels set `.row_highlight_style(...)` (models.rs:111, cost.rs:114, sources.rs:102, projects.rs:83, blocks.rs:113) but render with `render_widget` and NO TableState → highlight NEVER applies (dead style).

CHARTS/VIZ:
- stats.rs contribution heatmap: GitHub-style 7-row calendar of ■ (U+25A0), quantile buckets P25/50/75/99 (:502-522), heat ramp theme::heat (5 levels), "less ■■■■ more" legend (:262-284); compact single-row ./■ strip fallback (:287-313).
- stats.rs + hourly.rs source/hour ASCII bars via render_bar "#"/"-".
- panels/trends.rs: a FULL bar chart (axis, █ bars, value labels, peak highlight, compact ▁/· fallback) — but DEAD (not dispatched).
NUMBER FORMATTING: thousands separators via hand-rolled `format_number` DUPLICATED in ~11 files (overview/models/cost/daily/hourly/stats/sources/projects/behavior/blocks/usage) + `format_tokens` (k/M) + report_table `format_token_compact` (K/M/B) + cost "${:.2}"/"{:.4}". Percent "{:.0/1}%".
UNICODE: box-drawing (report_table.rs), ■ ▁ · █ ─ ↑ │ └─.
LANGUAGE: MIXED — several panel titles/placeholders Chinese (概览/模型/成本/区块/来源/项目/健康/趋势/行为, "加载中…", "数据加载失败", "暂无X数据"), others English (Daily Usage/Hourly Usage/Stats/Usage-Sync, "Loading…", "No … data"). Footer/help/overview labels mix EN + CN.
STATES: every panel matches None/Err/empty (e.g. models.rs:22-42). Selected tab highlight = nav only. Empty-state text is actionable ("Press r to refresh").
MINOR: footer controls text says "tab/shift-tab or 1-8 view" (footer.rs:52) but there are 9 panels/digits (help_dialog.rs:57 correctly says 1-9).

## 5. PERFORMANCE ISSUES

[P0] SYNC ON UI THREAD + NESTED TOKIO RUNTIME (likely panic). run_sync_action (mod.rs:179-207): builds `tokio::runtime::Builder::new_current_thread().build()` (:181-184) and `runtime.block_on(sync::run_store_once_with_options(...))` (:195). This executes on a thread already inside the outer `#[tokio::main]` multi-thread runtime (dash.rs runs the sync TUI loop on a runtime worker) → tokio's nested-runtime guard panics "Cannot start a runtime from within a runtime." Static-call-path analysis; appears uncovered by tests (no integration test drives the interactive `x` path; sync unit tests call the async fns directly under #[tokio::test]). Even if it did not panic, sync runs the ENTIRE import (parse all sources + DB writes) synchronously via block_on on the render thread — UI frozen (no redraw/input) for the whole sync.

[P1] ALL DB QUERIES SYNCHRONOUS ON RENDER THREAD. load_panel_data (mod.rs:248-323) runs queries inline in the key handler before the next draw → UI freezes for query duration on every first panel visit / refresh / source change. Heaviest: Blocks (full usage_event streaming scan, reports.rs:773-787), Behavior/Agents (5 payloads incl. multi-CTE tool_breakdown query/mod.rs:1305, 4 optimize detector scans :1442, filesystem zombie scan :1515), Health/Stats (5 queries incl. context_pressure raw-event GROUP BY :1000 and source_breakdown N+1 :1115-1127).

[P1] UNBOUNDED FULL-TABLE SCANS. Filter is always default (no since/until, filter.rs:44-55). overview/model_breakdown/cost_breakdown/trends_daily/source_breakdown = full GROUP BY over entire usage_bucket_30m; context_pressure/home_overview/blocks = full usage_event scans. Grows linearly with lifetime history; the "24h/Stats/Cost" views recompute lifetime aggregates every time.

[P2] REDRAW EVERY 250ms TICK REGARDLESS OF IDLE (mod.rs:97 + tick handler :101-107). No dirty flag; full re-render + full string re-formatting every frame. Every table rebuilds every visible row's `String` cells per frame (e.g. models.rs:61-98, daily.rs:57-100, hourly.rs:63-108, cost.rs:66-84) — per-frame allocations even when nothing changed.

[P2] THEME RWLOCK READ PER COLOR ACCESS. theme.rs:135-137 active_theme() acquires RwLock read + copies Theme; every accessor (accent/muted_fg/header_style/…) calls it. Hundreds of lock acquisitions per frame × 4 fps.

[P3] N+1 in source_breakdown (query/mod.rs:1115-1127): separate MAX(event_at) query per source (Stats panel).

[P3] EventHandler channel is unbounded std mpsc (event.rs:19); ticks queue if main thread is blocked (during a long query) → backlog of redraws processed after unblock.

### DEAD CODE / NO-OPS (load-bearing)

- DEAD PANEL MODULES (declared panels/mod.rs, never dispatched by draw.rs): panels/trends.rs, panels/sources.rs, panels/projects.rs, panels/health.rs. The Trends/Sources/Projects/Health enum variants render usage/daily/hourly/stats instead (draw.rs:27-63).
- DEAD CACHE FIELDS (app.rs): `trends`, `sources`, `projects`, `health` — only ever set to None (init + invalidate); never populated/read by the wired path. `project_breakdown()` query (query/mod.rs:1133) is never called by the TUI.
- DEAD TIME WINDOW: `time_window` is only written (mod.rs:151-161 NextWindow/PrevWindow) and read ONLY by the dead trends.rs. NextWindow/PrevWindow set trends=None (unused) then reload active panel, which is already cached and window-independent → h/l/←/→ have ZERO visible effect.
- DEAD SPINNER: `spinner_frame` incremented in on_tick (app.rs:405) but never rendered anywhere.
- DEAD row_highlight_style on 5 table panels (no TableState) — see §4.
- "Loading…"/"加载中…" None-branch placeholders effectively unreachable (data loaded before draw) — see §2.

## 6. EXISTING ASYNC / PARALLEL (reusable in-repo infra)

- web/mod.rs (axum server, the mature concurrent path the TUI does NOT use):
  - WebState (:49-66) holds Store + JobRegistry + `dashboard_query_semaphore: Arc<Semaphore>` (WEB_DASHBOARD_QUERY_PERMITS).
  - load_via_dashboard_with_timeout (:771-832): acquire semaphore permit w/ timeout → `tokio::task::spawn_blocking` (:817) opens a FRESH Dashboard per query, runs the closure off-runtime, exposes `dashboard.interrupt_handle()` (query/mod.rs:775) so a `tokio::time::timeout` can `interrupt()` a long SQLite query (:834-842). This is the exact offload+cancel pattern the TUI lacks.
  - Server started via `tokio::spawn(axum::serve …)` (:137-138).
- sync/job_registry.rs: full async job system — `JobRegistry` (DashMap + admission Mutex), `try_start/spawn_start` uses `tokio::spawn` (:175) running `run_job` (:278) which streams `SyncEvent` progress over `tokio::sync::mpsc` (:154, capacity 128), CancellationToken cancel, single-active-job admission, pollable JobSnapshot. commands/sync.rs run_store_once_with_options (:293) is the async sync entry (what the TUI wrongly wraps in a nested runtime).
- store/connection.rs:21-36 `open_connection` is cheap/repeatable (WAL, busy_timeout 30s) — multiple concurrent read connections are viable (web already opens one per query).
- The process already runs on a multi-thread tokio runtime (main.rs:1); Handle::current() is available on the TUI thread — a background query task via spawn_blocking + channel back to the loop is directly feasible without new infra.

## 7. TESTS

- NO snapshot tests, NO full-loop/integration tests of the interactive dashboard. No insta/golden files for TUI.
- Only ONE panel renders through a real backend: panels/blocks.rs:159-215 uses `ratatui::backend::TestBackend` (120×20), draws, and asserts on buffer text ("Burn/h","active","45,000"; empty/error/loading). This is the sole render-path test in the TUI.
- Pure-logic unit tests (no rendering): app.rs:421-540 proptest (panel nav, TimeWindow clamps, ScrollState bounds); input.rs:67-126; event.rs:66-87; nav_bar.rs:92-114; theme.rs:338-375; panels/stats.rs:595-670; panels/longtail.rs:69-105; report_table.rs:1031-1213 (CLI path).
- Data-layer tests live under query/* (home_overview.rs profiling harness :102-234) but none exercise the TUI.

## 勘误（2026-07-20 外部评审后追加，实现时以本节为准）

- §5 [P3] source_breakdown "N+1"：**误判**。per-source `MAX(event_at)`（query/mod.rs:1115-1127）是 dashboard-performance-contracts.md「Source totals」条款与 §7 Correct 示例明确要求的形态——走 `(source, event_at)` 索引，优于全表 GROUP BY。不是缺陷，不得"修复"。
- §2 Blocks 补充：块边界并非固定时钟窗。首块锚定于首个扫描事件的 `floor_to_hour`，后续事件按「是否 ≥ 前块 end」链式归属（reports.rs:768-787）；连续使用（相邻事件间隔 < 5h）时锚点链可上溯至全史首事件，任何截断扫描起点的收敛都必须处理锚点链。可证等值的截断点：相邻事件间隔 ≥ session_length（5h）的断档之后（后一事件必然 ≥ 前块 end，锚点链重置）。
- §6 补充：JobRegistry 是进程内内存态（docs/adr/0005-job-registry-in-memory.md），web 在 WebState::new 自建实例（web/mod.rs:57-59）；`JobState` 只含 snapshot + CancellationToken，无 JoinHandle（job_registry.rs:68-71）。`dash` 与 `serve` 为不同进程，跨进程并发 sync 由 worker lock 单写者契约裁决。
- §3/§4 补充：CLI 侧 `ColorMode::from_env`（report_table.rs:318-327）把 NO_COLOR 解释为 `Never`（完全无色）；TUI 引入无色支持须对齐该约定，与「受限色终端 RGB→ANSI16 降级」是两个独立机制。
- `Dashboard::project_breakdown` 仅 TUI 侧未调用；web `/api/projects`（web/mod.rs:250-259）与全量快照（query/mod.rs:2372）在用，非全局死代码。
