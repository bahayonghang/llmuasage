# tokscale TUI 参考模式分析（2026-07-20）

> 由并行分析 agent 产出的原始事实记录。仓库：`ref/repo/tokscale`（v4.5.3，ratatui 0.29 / crossterm 0.28）。
> 所有路径相对 tokscale 仓库根。核心启动/后台加载路径已由主会话亲自核验（tui/mod.rs）。

## 1. ARCHITECTURE

**Event loop: fully SYNCHRONOUS. No async in the TUI loop despite declared deps.**

- No `EventStream`, `tokio_stream`, `futures::`, `StreamExt`, `tokio::select`, `#[tokio::main]` anywhere in `crates/tokscale-cli/src`. The workspace `crossterm` `event-stream` feature (`Cargo.toml:61`), `tokio-stream` (`:64`), `futures` (`:65`) are declared but **unused by the TUI**. The async fns that exist are all reqwest network calls, each driven by ad-hoc `Runtime::new().block_on(...)` on背景线程.
- Main loop: `crates/tokscale-cli/src/tui/mod.rs:265` `run_loop_with_background` — plain `loop {}`. Per iteration: (1) unix SIGCONT re-enter alt screen after Ctrl-Z (`mod.rs:274-283`); (2) `terminal.draw(|f| ui::render(f, app))` EVERY iteration (`mod.rs:285`); (3) `bg_rx.try_recv()` for bg load result (`mod.rs:287-309`); (4) `app.needs_reload` → spawn new bg load thread (`mod.rs:311-335`); (5) **blocks** on `events.next()` = `rx.recv()` (`mod.rs:337`, `event.rs:64`).
- Event source: `event.rs:23` `EventHandler::new(100ms)` spawns ONE dedicated `std::thread` looping on blocking `crossterm::event::poll(tick_rate)` + `event::read()`, forwarding over `std::sync::mpsc`. Poll timeout → `Event::Tick`; also Tick on unmatched Focus events to avoid starvation (`event.rs:47-52`). Keys filtered to Press/Repeat (`event.rs:19-21`).
- Tick cadence ~100ms (10fps floor). `App::on_tick` (`app.rs:610`) advances spinner (`%20`), expires status after 3s, drives auto-refresh, polls all bg channels.
- **No loop-level dirty flag** — ratatui's double-buffer cell-diffing is the only redraw optimization.
- **Instant startup (cache-first)**: `tui::run` (`mod.rs:78`) calls `load_cache(...)` synchronously BEFORE the loop (`mod.rs:126`). `decide_initial_data` (`mod.rs:51`) returns cached data for both Fresh and Stale; `needs_background_load` is ALWAYS true. `App::new_with_cached_data` seeds `app.data`; a bg `std::thread::spawn` (`mod.rs:202`) runs `DataLoader::load`, saves cache, sends over `bg_tx`. Render shows the real tab (not spinner) whenever `background_loading` is true (`ui/mod.rs:43` shows spinner only if `data.loading && !background_loading`).

**Threading & channels — all std, no tokio tasks / no rayon in UI layer:**

- Bg data load: `std::thread::spawn` + `std::sync::mpsc::channel::<Result<UsageData>>` (`mod.rs:179,202,327`).
- Each bg feature has its own mpsc receiver polled in `on_tick`: `usage_rx` (`app.rs:962`), `codex_reset_rx` (`app.rs:1402`), `codex_login_rx` (`app.rs:1169`), `remote_stats_rx` (`app.rs:1077`) — all via `std::thread::spawn`.
- UI thread never blocks on data: blocks only on `events.next()` (wakes ≤100ms via Tick); drains bg channels with `try_recv`.
- `DataLoader::load` (`tui/data/mod.rs:320`) needs a tokio runtime for the async core parser; since it runs on a std thread it does `Runtime::new()?.block_on(...)`, and if a runtime is already current it uses a nested `std::thread::scope` with a fresh runtime (`data/mod.rs:349-361`) **to avoid runtime-in-runtime panics**（这正是 llmusage run_sync_action 缺的防护）.
- Panic hook restores terminal (`mod.rs:132`); `tokscale_core::tui_signal::set_tui_active(true)` gates bg stdio so cache warnings don't corrupt the alt screen (`mod.rs:154`).

## 2. DATA & CACHE LAYER

Two distinct caches: (A) TUI display cache (whole-snapshot JSON) + (B) core per-file incremental message cache (sharded bincode).

**(A) TUI display cache — `tui/cache.rs`:**

- Caches the entire aggregated `UsageData` snapshot (models, agents, daily, hourly, graph, totals, streaks) — NOT raw sessions. Plain JSON, `~/.cache/tokscale/tui-data-cache.json` (`cache.rs:69`).
- Key: `(enabled_clients set, include_synthetic, group_by, report_scope{since,until,year})`. `CACHE_SCHEMA_VERSION=10` (`cache.rs:24`).
- Staleness: `CACHE_STALE_THRESHOLD_MS=5min` (`cache.rs:23`). `load_cache` → Fresh/Stale/Miss (`cache.rs:712`). Schema downgrade or `ClientMatch::Subset` forces Stale (`cache.rs:802`).
- Client-set subset logic (`check_client_match`, `cache.rs:828`): cached ⊆ enabled → Subset (usable, refresh); superset/disjoint → Mismatch (miss).
- Atomic writes: temp `.{name}.{pid}.{nanos:x}.tmp` then rename; never deletes canonical first (`cache.rs:917-945`).
- `TUI_DEFAULT_GROUP_BY=GroupBy::Model` (`cache.rs:60`) single source of truth; documented past bug: warm-cache writer keyed differently from reader → cache missed every launch (regression tests `cache.rs:1943-2030`).

**(B) Core incremental message cache — `tokscale-core/message_cache.rs`:**

- The "only re-parse changed files" layer. `SourceMessageCache` (`:941`) = `HashMap<CacheKey, CachedSourceEntry>`; each entry = parser namespace+version, path, `SourceFingerprint`, parsed `Vec<UnifiedMessage>` for ONE file.
- Serialization: bincode. `CACHE_FORMAT_VERSION=2` (`:23`), 256 shards/parser-namespace (`:28`), shard file `shard-{index:02x}.bin` (`:1241`); envelope carries parser_version so a parser layout change can't brick other parsers' shards (`:927,1290-1306`).
- Fingerprint/invalidation (`SourceFingerprint`, `:127`): size+modified_ns primary (`:626`), plus optional sample_hashes / full content_hash per-parser.
- Concurrency: `fs2` shared lock on load (`:982`); dirty-tracking with `save_if_dirty` (`:1085`) writing only changed shards; corrupt shards quarantined+rewritten (`:1024-1030`).
- Background refresh cadence: default 30s, ±10s adjustable in 30–300s (`app.rs:412-418,2022-2048`), Shift-R toggles; manual `r`.
- Progressive surfacing: coarse — full `UsageData` snapshot swapped atomically via `App::update_data` (`app.rs:538`), bumps `data_version`, rebuilds shade map, invalidates memoized sort caches, re-anchors drill-down state by date.

## 3. PARALLELISM in core

- rayon everywhere in per-source parsing: `tokscale-core/lib.rs` `.par_iter()` at ~90 sites (1053,1123,1162,…). Pattern: each client's discovered path list parsed in parallel; each closure hits the incremental cache first, only parses on fingerprint miss.
- Merge is sequential + dedup: parallel results → serial `extend` filtering by `dedup_key` per-client `HashSet` (`lib.rs:1068-1078`). Deterministic because discovery returns sorted paths.
- SIMD JSON: `parser.rs:13` `simd_json::from_slice`; `parse_jsonl_file` (`:17`) line-by-line with reused mutable buffer, skipping malformed lines.
- Aggregation uses rayon map-reduce (`aggregator.rs:3,24,72`). Discovery: walkdir + `par_bridge()` (`scanner.rs:256-258`).
- **TUI-side aggregation is single-threaded**: `DataLoader::aggregate_messages` (`tui/data/mod.rs:421`) one sequential loop. Minutely bucketing gated behind `minutely_enabled` (`data/mod.rs:782`) — feature-gated expensive aggregation.
- release profile: lto=true, opt-level=3, codegen-units=1, strip=true.

## 4. FEATURES INVENTORY

**Tabs (9)** — `app.rs:55-138`: Overview, Usage, Models, Daily, Hourly, Minutely, Monthly, Stats, Agents. Minutely hidden unless enabled (`app.rs:1614`); cycling skips hidden.

**Keybindings** (`app.rs:772-937`): Ctrl-C/`q` quit; Tab/Shift-Tab & ←/→ cycle tabs; ↑/↓ select (wraps); PgUp/PgDn half-page; Home/End; sort `c`/`t`/`d` (re-press toggles direction, per-tab persisted `tab_sort_state` `app.rs:1641`, defaults date-desc for time tabs else cost-desc); `p` cycle theme; `s` source picker; `g` group-by picker; `r` refresh; Shift-R auto-refresh toggle; `+`/`-` interval; `y` copy row (arboard); `e` export JSON; context keys per tab (chart mode, drill-in Enter/Esc, jump-to-today `j`). Hotkeys normalized to US-QWERTY for non-Latin layouts (`app.rs:778`).

**Filtering**: source picker toggles enabled_clients (37 clients) → reload; group-by picker (Model/ClientModel/ClientProviderModel/WorkspaceModel/Session/ClientSession); date filters from CLI, part of cache key.

**Scroll/selection** (`app.rs:1646-1780`): manual `selected_index`+`scroll_offset`; renderers push viewport via `set_max_visible_items`; mouse left-click hit-tests `click_areas` (rebuilt each frame), wheel moves selection (`app.rs:1492-1527`).

**Drill-down**: Daily→per-source/per-model rows; Monthly→days-in-month; Stats graph cell→day breakdown. Detail state re-anchors by date on refresh.

**Persistence** (`settings.rs`, `~/.config/tokscale/settings.json`): colorPalette, autoRefresh, intervals, defaultClients, minutelyTabEnabled, modelAliases 等；主题/自动刷新即改即存。

**Other**: 12-theme switching, clipboard, JSON export, contribution graph, streaks, responsive (<80 narrow, <60 very narrow `app.rs:2488-2494`), warm-cache-after-submit detached subprocess (`main.rs:5454`).

## 5. UI STYLE INVENTORY

**Themes** (`themes.rs`): 12 (`ThemeName` `:48-62`): Green, Halloween, Teal, Blue, Pink, Purple, Orange, Monochrome, YlGnBu, Graphite, Lagoon, Dusk. Each = `colors:[Color;5]` 贡献图 5 级强度 ramp + background/foreground/border/highlight/muted/accent/selection/striped_row/current_row 语义槽位; surface themes 另覆写 bg/fg/rows (`:256-288`).

- `TerminalColorMode` (`:3`): FullColor vs Compatible from env (TERM/TERM_PROGRAM/COLORTERM/NO_COLOR); Apple_Terminal & NO_COLOR force Compatible, downgrading `Color::Rgb` → 16 ANSI via `compatible_rgb` (`:366-403`). All RGB passes through `theme.color()` (`:312`).
- Metric colors (`:319-333`): input green, output red, cacheR blue, cacheW orange.

**Model/provider color ramps** (`widgets.rs:167-360`, `colors.rs`): per-vendor 7-step ramps (Anthropic coral, OpenAI green, Google blue…); `build_model_shade_map` (`colors.rs:25`) ranks by family tier→version→cost→name; rebuilt only on data update (`app.rs:568`).

**Layout** (`ui/mod.rs:32-66`): header `Length(3)`, body `Min(0)`, footer `Length(5)`; dialog overlay last. Every frame `clear_click_areas`+rebuild (immediate-mode).

**Footer** (`footer.rs`, 3 rows): (1) sort buttons Date/Cost/Tokens (active bold, click areas) + right totals; (2) contextual keybind hints incl `[g:<groupby>]`,`[p:<theme>]`,`[R:auto Ns]`; (3) data-source label + spinner-or-status-or-"Last updated: Ns ago".

**Custom widgets**:

- Stacked bar chart (`bar_chart.rs`): direct buffer writes, 8-level sub-cell blocks `▁▂▃▄▅▆▇█` (`:7`); per-row max-overlap model color stacking (`:192`); y-axis label + axes; responsive x-labels.
- Contribution graph (`stats.rs:62-193`): 52wk×7d, `██` 2-wide cells, 5-grade intensity; selected `▓▓`; empty `· `; month/weekday labels; per-cell click areas; "Less ██ ██ ██ ██ More" legend.
- Scanner spinner (`spinner.rs`): KITT-style bouncing 8-cell trail `■`/`⬝` with 4-step fade (`:23-82`).
- Ratio bars (`widgets.rs:112`): `█` fill + `·` track + `▏` trace for tiny nonzero.
- Profile bars (`hourly_profile.rs`): `█`/`░` histograms.

**Tables** (canonical `models.rs`):

- **Manual windowing (not TableState scroll)**: slices `models[start..end]` (`:144`) so only visible rows build Strings; selection bg per-row (`idx==selected_index` → `theme.selection` `:227`), striping `idx%2==1`.
- Responsive cols: very-narrow 2 (Model,Cost), narrow 3, full 13; widths fixed `Length` numerics + `Min(20)` name (`:239-279`).
- Number formatting (`widgets.rs:9-89`): `format_tokens` K/M/B compact; commas; `format_cost` $X.XX/$X.XK; cost_per_million; per-col metric colors; sort indicator ▲/▼ on active header (`:101-110`); right scrollbar ▲/▼ + thumb █ (`:287`, `viewport_scrollbar_state` `widgets.rs:91`).

**Loading/empty/error** (`ui/mod.rs:68-123`): centered spinner ("Scanning session data..."), red error paragraph, per-tab empty hints; bg refresh with existing data → footer "Refreshing cached data in background...".

**Dialogs** (`ui/dialog/`): trait `DialogContent{desired_size,render,handle_key,handle_mouse}->DialogResult{None,Close,Replace}`; `DialogStack` overlay; reload signalled via shared `Rc<RefCell<bool>>`.

## 6. PATTERNS WORTH PORTING（可移植模式清单）

1. Cache-first instant render: load display cache pre-loop, seed App, always kick bg refresh, render real view (not spinner) when cache exists. `mod.rs:126-212`, `ui/mod.rs:43`.
2. Sync threaded loop: crossterm-poll thread → mpsc → `recv()` loop with Tick timeout; bg work on threads + mpsc, drained `try_recv` in tick. No async in render path. `event.rs:23`, `mod.rs:265-357`.
3. Snapshot swap + `data_version` counter invalidating memoized derived state. `app.rs:538,2372-2445`.
4. Two-tier caching: whole-snapshot display cache over per-file incremental cache. `tui/cache.rs` + `message_cache.rs`.
5. Per-file fingerprint cache with parser-version isolation (size+mtime_ns primary; 256 bincode shards). `message_cache.rs:127,626,880,927`.
6. rayon `par_iter` per source file + serial dedup merge, cache-gated closures. `lib.rs:1051,1123,1162`.
7. simd-json whole-file + JSONL reused buffer, skip malformed. `parser.rs:13,17`.
8. Atomic cache writes (temp+rename) `cache.rs:917`; `fs2` shared lock `message_cache.rs:982`.
9. Immediate-mode click areas: clear+rebuild `Vec<ClickArea>` each frame, hit-test on mouse-down. `app.rs:1563-1568,1492-1518`.
10. Manual row windowing (`data[start..end]`) instead of feeding whole dataset to ratatui. `models.rs:144`.
11. Precomputed model→color map rebuilt only on data update. `app.rs:568`, `colors.rs:25`.
12. Terminal color-mode downgrade (RGB→ANSI) gated on env (NO_COLOR / Apple_Terminal). `themes.rs:290-307,366`.
13. Sub-cell block-glyph charts (`▁▂▃▄▅▆▇█`, `██`/`▓▓`). `bar_chart.rs:7`, `stats.rs:145`.
14. Per-tab persisted sort + date-reanchored drill-down surviving refresh. `app.rs:1641,1920-1998`.
15. Warm-cache-after-write detached subprocess so next launch is instant. `main.rs:5454,5582`.
16. Panic hook + SIGCONT handling to always restore terminal / re-enter alt screen. `mod.rs:132,274`.
17. Feature-gated expensive aggregation (minutely only when tab enabled). `data/mod.rs:782`.
18. Cache-key subset tolerance so an added client doesn't force cold start. `cache.rs:828`.

## 7. 与 llmusage 的适配判断（主会话结论）

- llmusage 数据层是 SQLite（usage_bucket_30m 预聚合），天然等价于 tokscale 的两级缓存的下层——**不需要**引入 bincode 分片缓存或磁盘快照缓存；瓶颈不在解析而在「查询跑在渲染线程 + 恒全表扫描」。
- llmusage 进程已运行在 multi-thread tokio runtime 内，且 web/mod.rs 已有 spawn_blocking + Semaphore + InterruptHandle 的契约化查询模式（dashboard-performance-contracts.md §3）——TUI 异步化应复用该模式，而非照抄 tokscale 的 std::thread 方案；但「try_recv 排水 + loading 态 + 世代丢弃」的循环结构可直接对标。
- tokscale 的 rayon/simd-json 在解析核心层，llmusage 对应层是 sync/parsers（已有独立任务线：07-20-sync-cold-import-write-throughput）；本父任务不覆盖解析层并行化。
