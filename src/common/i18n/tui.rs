//! Paired TUI presentation catalog. IDs, data and numeric formatting stay neutral.
use super::current;

pub(crate) fn text(id: &str) -> &'static str {
    let (english, chinese) = match id {
        "overview" => ("Overview", "概览"),
        "usage" => ("Usage", "额度"),
        "models" => ("Models", "模型"),
        "daily" => ("Daily", "每日"),
        "hourly" => ("Hourly", "每时"),
        "monthly" => ("Monthly", "每月"),
        "stats" => ("Stats", "统计"),
        "agents" => ("Agents", "智能体"),
        "blocks" => ("Blocks", "时段"),
        "ovw" => ("Ovw", "概"),
        "use" => ("Use", "额"),
        "mod" => ("Mod", "模"),
        "day" => ("Day", "日"),
        "hr" => ("Hr", "时"),
        "mon" => ("Mon", "月"),
        "sta" => ("Sta", "统"),
        "agt" => ("Agt", "智"),
        "blk" => ("Blk", "段"),
        "today" => ("Today", "今天"),
        "7d" => ("7d", "7天"),
        "30d" => ("30d", "30天"),
        "all" => ("All", "全部"),
        "date" => ("date", "日期"),
        "tokens" => ("tokens", "令牌"),
        "cost" => ("cost", "费用"),
        "emails_hidden" => ("Emails hidden", "邮箱已隐藏"),
        "emails_visible" => ("Emails visible", "邮箱已显示"),
        "source_filter_cleared" => ("Source filter cleared", "已清除来源筛选"),
        "auto_refresh_enabled" => ("Auto refresh enabled", "已开启自动刷新"),
        "auto_refresh_disabled" => ("Auto refresh disabled", "已关闭自动刷新"),
        "tab_1_9_view_j_k_row_pg_page_o_o_sort_s_source_r_refresh_x_sync_q" => (
            "tab/1-9 view  j/k row  Pg page  o/O sort  s source  r refresh  x sync  ? q",
            "tab/1-9 视图  j/k 选行  Pg 翻页  o/O 排序  s 来源  r 刷新  x 同步  ? q",
        ),
        "tab_shift_tab_or_1_9_view" => {
            ("tab/shift-tab or 1-9 view", "tab/shift-tab 或 1-9 切换视图")
        }
        "j_k_pg_select" => ("[j/k/Pg:select]", "[j/k/Pg:选择]"),
        "o_o_sort" => ("[o/O:sort]", "[o/O:排序]"),
        "esc_back" => ("[esc:back]", "[esc:返回]"),
        "enter_detail" => ("[enter:detail]", "[enter:详情]"),
        "s_source" => ("[s:source]", "[s:来源]"),
        "r_refresh" => ("[r:refresh]", "[r:刷新]"),
        "x_sync" => ("[x:sync]", "[x:同步]"),
        "y_status" => ("[y:status]", "[y:状态]"),
        "m_mail" => ("[m:mail]", "[m:邮箱]"),
        "r_auto_on" => ("[R:auto on]", "[R:自动开]"),
        "r_auto_off" => ("[R:auto off]", "[R:自动关]"),
        "source" => ("source ", "来源 "),
        "window" => ("window ", "范围 "),
        "local_dashboard_cache" => ("local dashboard cache", "本地看板缓存"),
        "help_settings" => (" Help / Settings ", " 帮助 / 设置 "),
        "theme" => ("Theme ", "主题 "),
        "source_2" => (" • Source ", " • 来源 "),
        "window_2" => (" • Window ", " • 范围 "),
        "tab_shift_tab_switch_tabs_1_9_jump_to_tab" => (
            "tab / shift-tab: switch tabs    1-9: jump to tab",
            "tab / shift-tab: 切换视图    1-9: 跳转视图",
        ),
        "j_k_or_arrows_select_row_pgup_pgdn_page" => (
            "j/k or arrows: select row        PgUp/PgDn: page",
            "j/k 或方向键: 选行             PgUp/PgDn: 翻页",
        ),
        "home_end_first_last_row_o_o_sort_key_direction" => (
            "Home/End: first/last row          o/O: sort key/direction",
            "Home/End: 首行/末行            o/O: 排序列/方向",
        ),
        "mouse_wheel_select_row_h_l_today_7d_30d_all" => (
            "mouse wheel: select row           h/l: Today/7d/30d/All",
            "鼠标滚轮: 选行                h/l: 今天/7天/30天/全部",
        ),
        "window_models_daily_hourly_monthly_stats_behavior" => (
            "window: Models/Daily/Hourly/Monthly/Stats/Behavior",
            "范围: 模型/每日/每时/每月/统计/行为",
        ),
        "enter_day_month_detail_esc_back_quit" => (
            "enter: day/month detail           Esc: back / quit",
            "enter: 日/月详情               Esc: 返回 / 退出",
        ),
        "stats_click_day_enter_today_esc_close" => (
            "Stats: click day / Enter today / Esc close",
            "统计: 点击日期 / Enter 今天 / Esc 关闭",
        ),
        "s_source_picker_a_all_sources_in_picker" => (
            "s: source picker                 a: all sources in picker",
            "s: 来源选择                   a: 全部来源",
        ),
        "r_refresh_dashboard_cache_r_toggle_auto_refresh" => (
            "r: refresh dashboard cache       R: toggle auto refresh",
            "r: 刷新看板缓存               R: 开关自动刷新",
        ),
        "x_run_sync_for_current_source_y_sync_status_overlay" => (
            "x: run sync for current source   y: sync status overlay",
            "x: 同步当前来源               y: 同步状态覆盖层",
        ),
        "m_show_hide_usage_emails_t_cycle_theme" => (
            "m: show/hide Usage emails        t: cycle theme",
            "m: 显示/隐藏邮箱              t: 切换主题",
        ),
        "this_dialog_q_or_esc_close_quit" => (
            "?: this dialog                    q or Esc: close / quit",
            "?: 本对话框                   q 或 Esc: 关闭 / 退出",
        ),
        "parserless_platforms_remain_monitor_only_until_fixtures_and_token_semantics_exist" => (
            "Parserless platforms remain monitor-only until fixtures and token semantics exist.",
            "缺少解析器的平台仅监测，需先具备样本和令牌语义。",
        ),
        "sources" => (" Sources ", " 来源 "),
        "source_picker" => ("Source picker", "来源选择"),
        "enter_select_a_all_sources_esc_close" => (
            "Enter select • a all sources • Esc close",
            "Enter 选择 • a 全部来源 • Esc 关闭",
        ),
        "no_source_monitors" => ("No source monitors", "没有来源监测器"),
        "descending" => ("descending", "降序"),
        "ascending" => ("ascending", "升序"),
        "sync_cancelled" => ("Sync cancelled", "同步已取消"),
        "refreshing_local_dashboard_cache" => {
            ("Refreshing local dashboard cache", "正在刷新本地看板缓存")
        }
        "no_subscription_credentials_found" => {
            ("No subscription credentials found", "未找到订阅凭据")
        }
        "refreshed_local_dashboard_cache" => {
            ("Refreshed local dashboard cache", "已刷新本地看板缓存")
        }
        "local_usage" => (" local usage ", " 本地用量 "),
        "sync_cancellation_requested" => ("Sync cancellation requested", "已请求取消同步"),
        "sync_job_is_no_longer_available" => {
            ("Sync job is no longer available", "同步任务已不可用")
        }
        "sync_running_press_x_to_cancel" => {
            ("Sync running... press x to cancel", "同步进行中…按 x 取消")
        }
        "sync_started" => ("Sync started", "同步已开始"),
        "preparing_local_database" => ("Preparing local database", "正在准备本地数据库"),
        "waiting_for_sync_worker_lock" => ("Waiting for sync worker lock", "正在等待同步工作锁"),
        "replaying" => ("replaying", "重放"),
        "scanning" => ("scanning", "扫描"),
        "loading" => ("Loading...", "加载中…"),
        "behavior" => ("Behavior", "行为"),
        "no_activity_behavior_facts" => ("No activity behavior facts.", "没有活动行为数据。"),
        "behavior_activity_categories" => ("Behavior / Activity Categories", "行为 / 活动分类"),
        "no_tool_behavior_facts" => ("No tool behavior facts.", "没有工具行为数据。"),
        "behavior_tools" => ("Behavior / Tools", "行为 / 工具"),
        "read_only_advice_llmusage_never_deletes_archives_rewrites_or_cleans_content_automatically" => {
            (
                "Read-only advice: llmusage never deletes, archives, rewrites, or cleans content automatically.",
                "只读建议：llmusage 从不自动删除、归档、重写或清理内容。",
            )
        }
        "no_behavior_facts_score_and_savings_are_not_calculated" => (
            "No behavior facts; score and savings are not calculated.",
            "没有行为数据；不计算评分和节省量。",
        ),
        "no_obvious_waste_patterns_found_review_the_surrounding_context_manually" => (
            "No obvious waste patterns found; review the surrounding context manually.",
            "未发现明显浪费模式；请人工检查相关上下文。",
        ),
        "behavior_optimize_read_only" => ("Behavior / Optimize (read-only)", "行为 / 优化（只读）"),
        "warning" => ("Warning ", "警告 "),
        "higher_better" => (" (higher better)", "（越高越好）"),
        "compare_requires_at_least_two_models_with_local_usage" => (
            "Compare requires at least two models with local usage.",
            "对比至少需要两个有本地用量的模型。",
        ),
        "behavior_model_comparison" => ("Behavior / Model Comparison", "行为 / 模型对比"),
        "status" => ("Status ", "状态 "),
        "no_block_data_found" => ("No block data found.", "没有时段数据。"),
        "window_3" => ("Window", "范围"),
        "status_2" => ("Status", "状态"),
        "total_tokens" => ("Total Tokens", "总令牌"),
        "burn_h" => ("Burn/h", "消耗/时"),
        "projected" => ("Projected", "预计"),
        "limit" => ("Limit", "上限"),
        "cost_usd" => ("Cost (USD)", "费用 (USD)"),
        "active" => ("active", "活跃"),
        "blocks_5h_burn_rate" => ("Blocks (5h burn rate)", "时段（5小时消耗速率）"),
        "daily_usage" => ("Daily Usage", "每日用量"),
        "no_daily_usage_data_found_press_r_to_refresh" => (
            "No daily usage data found. Press r to refresh.",
            "没有每日用量数据。按 r 刷新。",
        ),
        "date_2" => ("Date", "日期"),
        "no_model_details_found_for_this_day_press_esc_to_go_back" => (
            "No model details found for this day. Press Esc to go back.",
            "当天没有模型详情。按 Esc 返回。",
        ),
        "model" => ("Model", "模型"),
        "cost_2" => ("Cost", "费用"),
        "source_3" => ("Source", "来源"),
        "msgs" => ("Msgs", "消息"),
        "tokens_3" => ("Tokens", "令牌"),
        "input" => ("Input", "输入"),
        "output" => ("Output", "输出"),
        "cache_r" => ("Cache R", "缓存读"),
        "cache_w" => ("Cache W", "缓存写"),
        "cache" => ("Cache×", "缓存×"),
        "total" => ("Total", "合计"),
        "cost_1m" => ("Cost/1M", "费用/百万"),
        "hourly_usage" => ("Hourly Usage", "每时用量"),
        "no_hourly_usage_data_found_press_r_to_refresh" => (
            "No hourly usage data found. Press r to refresh.",
            "没有每时用量数据。按 r 刷新。",
        ),
        "hour" => ("Hour", "小时"),
        "no_model_data_found" => ("No model data found.", "没有模型数据。"),
        "provider" => ("Provider", "提供商"),
        "events" => ("Events", "事件"),
        "monthly_usage" => ("Monthly Usage", "每月用量"),
        "no_monthly_usage_data_found_press_r_to_refresh" => (
            "No monthly usage data found. Press r to refresh.",
            "没有每月用量数据。按 r 刷新。",
        ),
        "month" => ("Month", "月份"),
        "tokens_per_day" => ("Tokens per Day", "每日令牌"),
        "top_models" => ("Top Models", "热门模型"),
        "models_by_tokens" => ("Models by Tokens", "模型令牌排行"),
        "models_by_cost" => ("Models by Cost", "模型费用排行"),
        "in" => ("  In: ", "  输入："),
        "out" => (" · Out: ", " · 输出："),
        "cr" => (" · CR: ", " · 缓存读："),
        "cw" => (" · CW: ", " · 缓存写："),
        "turn" => ("Turn", "轮次"),
        "no_heatmap_data" => ("No heatmap data", "没有热力图数据"),
        "graph_is_too_narrow" => ("Graph is too narrow", "图表宽度不足"),
        "model_2" => ("Model:", "模型："),
        "favorite_model" => ("Favorite model:", "最常用模型："),
        "n_a" => ("N/A", "无"),
        "tokens_4" => ("Tokens:", "令牌："),
        "total_tokens_2" => ("Total tokens:", "总令牌："),
        "events_2" => ("Events:", "事件："),
        "cost_3" => ("Cost:", "费用："),
        "total_cost" => ("Total cost:", "总费用："),
        "streak" => ("Streak:", "连续天数："),
        "current_streak" => ("Current streak:", "当前连续天数："),
        "max_streak" => ("Max streak:", "最长连续："),
        "longest_streak" => ("Longest streak:", "最长连续天数："),
        "active_2" => ("Active:", "活跃："),
        "active_days" => ("Active days:", "活跃天数："),
        "no_data_for_this_day" => ("No data for this day", "当天没有数据"),
        "in_2" => ("In · ", "输入 · "),
        "out_2" => (" · Out · ", " · 输出 · "),
        "cr_2" => (" · CR · ", " · 缓存读 · "),
        "cw_2" => (" · CW · ", " · 缓存写 · "),
        "context" => ("Context", "上下文"),
        "n_a_2" => ("n/a", "无"),
        "context_peak" => ("Context peak ", "上下文峰值 "),
        "avg" => ("  avg ", "  平均 "),
        "less" => ("Less ", "少 "),
        "more" => (" More", " 多"),
        "contribution_graph_52_weeks" => ("Contribution Graph (52 weeks)", "贡献图（52周）"),
        "day_breakdown_esc_to_close" => ("Day Breakdown (ESC to close)", "每日明细（ESC 关闭）"),
        "usage_sync" => ("Usage / Sync", "额度 / 同步"),
        "events_3" => ("events ", "事件 "),
        "inserted" => ("  inserted ", "  新增 "),
        "stored" => ("  stored ", "  存储 "),
        "sources_2" => ("  sources ", "  来源 "),
        "lock" => ("lock ", "锁 "),
        "rebuild_risk" => ("  rebuild-risk ", "  重建风险 "),
        "monitored" => ("  monitored ", "  已监测 "),
        "parserless" => ("  parserless ", "  无解析器 "),
        "last_run_none_recorded" => ("last run: none recorded", "上次运行：无记录"),
        "rebuild_risk_facts" => ("rebuild-risk facts ", "重建风险事实 "),
        "no_source_sync_status_yet_run_sync_or_refresh_after_imports" => (
            "No source sync status yet. Run sync or refresh after imports.",
            "暂无来源同步状态。导入后运行同步或刷新。",
        ),
        "source_sync" => ("Source Sync", "来源同步"),
        "monitor_only_detected_none" => ("monitor-only detected: none", "仅监测平台：未检测到"),
        "platform_probes" => ("platform probes ", "平台探测 "),
        "blocked_no_samples" => ("  blocked-no-samples ", "  缺少样本 "),
        "parserless_platforms_stay_monitor_only_until_fixtures_and_token_semantics_exist" => (
            "parserless platforms stay monitor-only until fixtures and token semantics exist",
            "缺少解析器的平台仅监测，需先具备样本和令牌语义",
        ),
        "platform_monitor" => ("Platform Monitor", "平台监测"),
        "stored_2" => ("Stored", "存储"),
        "seen" => ("Seen", "已见"),
        "inserted_2" => ("Inserted", "新增"),
        "skipped" => ("Skipped", "跳过"),
        "issues" => ("Issues", "问题"),
        "share" => ("Share", "占比"),
        "updated" => ("Updated", "更新"),
        "last_run" => ("last run ", "上次运行 "),
        "started" => ("  started ", "  开始于 "),
        "ready_to_sync" => ("Ready to sync", "可同步"),
        "sync_is_running" => ("Sync is running", "同步进行中"),
        "recent_sync_failed" => ("Recent sync failed", "最近同步失败"),
        "rebuild_risk_2" => ("Rebuild risk", "重建风险"),
        "sync_status" => ("Sync status", "同步状态"),
        "local_status_looks_usable" => ("local status looks usable", "本地状态可用"),
        "no_sync_status_has_been_recorded" => {
            ("no sync status has been recorded", "暂无同步状态记录")
        }
        "the_latest_usage_import_failed" => {
            ("the latest usage import failed", "最近一次用量导入失败")
        }
        "missing_source_files_protect_stored_history" => (
            "missing source files protect stored history",
            "缺失来源文件的已存历史受保护",
        ),
        "check_sync_details_below" => ("check sync details below", "请检查下方同步详情"),
        "yes" => ("yes", "是"),
        "no" => ("no", "否"),
        _ => unreachable!("unknown TUI message id: {id}"),
    };
    current().select(english, chinese)
}

macro_rules! format {
    (@pair $en:literal, $zh:literal, $($args:tt)*) => {
        match $crate::common::i18n::current() {
            $crate::common::i18n::Language::English => std::format!($en, $($args)*),
            $crate::common::i18n::Language::Chinese => std::format!($zh, $($args)*),
        }
    };
    (source_filter $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Source filter: {}", "来源筛选：{}", $($($args)*)?) };
    (is_monitor_only $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{} is monitor-only: {}", "{} 仅监测：{}", $($($args)*)?) };
    (tokens_2 $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{} tokens • ${:.2}", "{} 令牌 • ${:.2}", $($($args)*)?) };
    (filter $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Filter: {}", "筛选：{}", $($($args)*)?) };
    (sort $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Sort: {} {}", "排序：{} {}", $($($args)*)?) };
    (theme_name $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "theme: {name}", "主题：{name}", $($($args)*)?) };
    (sync_complete_inserted_inserted_stored_stored $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Sync complete: {inserted} inserted, {stored} stored", "同步完成：新增 {inserted}，已存储 {stored}", $($($args)*)?) };
    (sync_failed_error $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Sync failed: {error}", "同步失败：{error}", $($($args)*)?) };
    (loaded_quota_account_s $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Loaded {} quota account(s)", "已加载 {} 个额度账户", $($($args)*)?) };
    (loaded_quota_account_s_issue_s $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Loaded {} quota account(s), {} issue(s)", "已加载 {} 个额度账户，{} 个问题", $($($args)*)?) };
    (sync_start_rejected_err $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Sync start rejected: {err}", "同步启动被拒绝：{err}", $($($args)*)?) };
    (migrating_database_v_version_latest_version $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Migrating database v{version}/{latest_version}", "正在迁移数据库 v{version}/{latest_version}", $($($args)*)?) };
    (database_migration_v_version_complete_elapsed_ms_ms $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Database migration v{version} complete ({elapsed_ms}ms)", "数据库迁移 v{version} 完成（{elapsed_ms}ms）", $($($args)*)?) };
    (updating_prices_for_total_events_events $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Updating prices for {total_events} events", "正在更新 {total_events} 个事件的价格", $($($args)*)?) };
    (updating_prices_processed_events_total_events $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Updating prices {processed_events}/{total_events}", "正在更新价格 {processed_events}/{total_events}", $($($args)*)?) };
    (reconciling_bucket_count_pricing_buckets $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Reconciling {bucket_count} pricing buckets", "正在核对 {bucket_count} 个计价桶", $($($args)*)?) };
    (prices_updated_updated_events_events_bucket_count_buckets $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Prices updated: {updated_events} events, {bucket_count} buckets", "价格已更新：{updated_events} 个事件，{bucket_count} 个桶", $($($args)*)?) };
    (sync_worker_lock_acquired_after_wait_ms_ms $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Sync worker lock acquired after {wait_ms}ms", "{wait_ms}ms 后已取得同步工作锁", $($($args)*)?) };
    (repairing_legacy_token_accounting $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Repairing legacy token accounting: {}", "正在修复旧版令牌统计：{}", $($($args)*)?) };
    (token_accounting_repair_complete $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Token accounting repair complete: {}", "令牌统计修复完成：{}", $($($args)*)?) };
    (action_files_total_files $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{}: {action} {files_total} files", "{}：{action} {files_total} 个文件", $($($args)*)?) };
    (files_scanned_files_replayed $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{files_scanned} files replayed", "已重放 {files_scanned} 个文件", $($($args)*)?) };
    (files_scanned_rows_scanned $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{files_scanned} rows scanned", "已扫描 {files_scanned} 行", $($($args)*)?) };
    (work_records_imported_records $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{}: {work}, {records_imported} records", "{}：{work}，{records_imported} 条记录", $($($args)*)?) };
    (recent_usage_ready $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{}: recent usage ready", "{}：近期用量已就绪", $($($args)*)?) };
    (files_skipped_stored $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{}: {} files, {} skipped, {} stored", "{}：{} 个文件，跳过 {}，存储 {}", $($($args)*)?) };
    (importing_remote_host_label $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Importing remote host {label}", "正在导入远程主机 {label}", $($($args)*)?) };
    (remote_host_label_imported_sources $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Remote host {label} imported ({} sources)", "远程主机 {label} 已导入（{} 个来源）", $($($args)*)?) };
    (skipped_remote_host_label_reason $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Skipped remote host {label}: {reason}", "已跳过远程主机 {label}：{reason}", $($($args)*)?) };
    (sync_complete_inserted_stored $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Sync complete: {} inserted, {} stored", "同步完成：新增 {}，存储 {}", $($($args)*)?) };
    (data_load_failed_e $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Data load failed: {e}", "数据加载失败：{e}", $($($args)*)?) };
    (turns_edits_tokens_cost_one_shot_retry $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "turns={} edits={} tokens={} cost={} one-shot={} retry={}", "轮次={} 编辑={} 令牌={} 费用={} 一次完成={} 重试={}", $($($args)*)?) };
    (calls_turns_sessions_share_cost $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "calls={} turns={} sessions={} share={} cost={}", "调用={} 轮次={} 会话={} 占比={} 费用={}", $($($args)*)?) };
    (score $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Score {} ({}) ", "评分 {} ({}) ", $($($args)*)?) };
    (estimated_savings $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "estimated savings={} / {}", "预计节省={} / {}", $($($args)*)?) };
    (recommendation $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{}. Recommendation: {}", "{}。建议：{}", $($($args)*)?) };
    (zombie_skills_mcps_none_installed_items_scanned $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Zombie skills/MCPs: none ({} installed items scanned)", "未调用技能/MCP：无（已扫描 {} 个安装项）", $($($args)*)?) };
    (zombies $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Zombies {} ", "未调用项 {} ", $($($args)*)?) };
    (installed_but_never_called_installed_cleanup_candidates_llmusage_never_deletes_automatically $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Installed but never called ({} installed): {} (cleanup candidates; llmusage never deletes automatically)", "已安装但从未调用（共 {} 项）：{}（清理候选；llmusage 从不自动删除）", $($($args)*)?) };
    (vs $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{} vs {} ", "{} 对比 {} ", $($($args)*)?) };
    (calls_edits_cost $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "calls {}:{} / {}:{} edits {}:{} / {}:{} cost {}:{} / {}:{}", "调用 {}:{} / {}:{} 编辑 {}:{} / {}:{} 费用 {}:{} / {}:{}", $($($args)*)?) };
    (category $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "category:{} ", "分类：{} ", $($($args)*)?) };
    (one_shot_vs $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "one-shot {} vs {}", "一次完成 {} 对比 {}", $($($args)*)?) };
    (candidate_models $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Candidate models: {}", "候选模型：{}", $($($args)*)?) };
    (daily_detail_date $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Daily Detail: {date}", "每日详情：{date}", $($($args)*)?) };
    (daily_breakdown_month $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Daily Breakdown: {month}", "每日明细：{month}", $($($args)*)?) };
    (total_2 $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Total: {}", "合计：{}", $($($args)*)?) };
    (current_days $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{current} days", "{current} 天", $($($args)*)?) };
    (longest_days $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{longest} days", "{longest} 天", $($($args)*)?) };
    (data_load_failed_error $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "Data load failed: {error}", "数据加载失败：{error}", $($($args)*)?) };
    (model_count_model_plural $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair " ({model_count} model{plural})", "（{model_count} 个模型{plural}）", $($($args)*)?) };
    (detected_detected $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{detected}/{} detected", "已检测 {detected}/{}", $($($args)*)?) };
    (missing_protected $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "{} missing={} protected={}", "{} 缺失={} 受保护={}", $($($args)*)?) };
    (monitor_only_detected $(, $($args:tt)*)?) => { $crate::common::i18n::tui::format!(@pair "monitor-only detected: {}", "仅监测平台：{}", $($($args)*)?) };
}
pub(crate) use format;

pub(crate) fn original(detail: &str) -> String {
    std::format!("{} {detail}", super::text("Original detail:", "原始详情："))
}

/// Stable values are projected only at the presentation boundary.
pub(crate) fn value(id: &str) -> &str {
    let zh = match id {
        "normalized" => "标准化",
        "no_data" => "无数据",
        "low_sample" => "样本不足",
        "insufficient_models" => "模型不足",
        "missing_model" => "模型缺失",
        "high" => "高",
        "medium" => "中",
        "low" => "低",
        "available" => "可用",
        "busy" => "忙碌",
        "running" => "运行中",
        "ready" => "就绪",
        "stale" => "待刷新",
        "idle" => "空闲",
        "ok" => "正常",
        "never" => "未运行",
        "absent" => "不存在",
        "error" => "错误",
        "success" => "成功",
        "failed" => "失败",
        "cancelled" => "已取消",
        "detected" => "已检测",
        "unavailable" => "不可用",
        "registered" => "已注册",
        "planned" => "已规划",
        "blocked_no_samples" => "缺少样本",
        "blocked_no_usage" => "缺少用量",
        "unsupported" => "不支持",
        "external_only" => "仅外部",
        "precise" => "精确",
        "estimated" => "估算",
        "partial" => "部分",
        "malformed" => "格式错误",
        "oversized" => "超长",
        "skipped" => "跳过",
        "accounting" => "计数提示",
        "coding" => "编程",
        "exploration" => "探索",
        "planning" => "规划",
        "delegation" => "委派",
        "other" => "其他",
        "read" => "读取",
        "search" => "搜索",
        "edit" => "编辑",
        "write" => "写入",
        "shell" => "终端",
        "skill" => "技能",
        _ => return id,
    };
    super::text(id, zh)
}

pub(crate) fn support_reason(level: &str, detail: Option<&str>) -> Option<String> {
    let pair = match level {
        "no_data" => (
            "No normalized behavior facts match this filter; run sync with a parser that emits behavior facts.",
            "没有匹配筛选的标准化行为数据；请使用产生行为数据的解析器运行同步。",
        ),
        "insufficient_models" => (
            "At least two models with local usage are required for comparison.",
            "对比至少需要两个有本地用量的模型。",
        ),
        "low_sample" => (
            "Low sample: compare directionally until each model has more calls/edit turns.",
            "样本不足：每个模型有更多调用/编辑轮次后再做可靠对比；当前仅供方向参考。",
        ),
        "missing_model" => (
            "One selected model has no data in this filter.",
            "一个所选模型在当前筛选下没有数据。",
        ),
        _ => return detail.map(original),
    };
    detail.map(|_| super::text(pair.0, pair.1).to_string())
}

pub(crate) fn metric_label<'a>(id: &str, fallback: &'a str) -> &'a str {
    let pair = match id {
        "one_shot_rate" => ("One-shot rate", "一次完成率"),
        "retry_rate" => ("Retry rate", "重试率"),
        "cost_per_call" => ("Cost / call", "费用 / 调用"),
        "cost_per_edit_turn" => ("Cost / edit", "费用 / 编辑"),
        "cache_efficiency" => ("Cache efficiency", "缓存效率"),
        "delegation_rate" => ("Delegation", "委派率"),
        "planning_rate" => ("Planning", "规划率"),
        "tools_per_turn" => ("Tools / turn", "工具 / 轮次"),
        _ => return fallback,
    };
    super::text(pair.0, pair.1)
}

pub(crate) fn finding(
    finding: &crate::query::OptimizeFinding,
    evidence: Option<&crate::query::OptimizeEvidence>,
) -> (String, String, String) {
    use crate::query::OptimizeEvidence as E;
    let pair = match finding.id.as_str() {
        "low_read_edit_ratio" => (
            "读取/编辑比偏低",
            "在较大编辑前先检查文件；这是只读信号，不会自动重写。",
        ),
        "duplicate_reads" => (
            "同一会话中重复读取",
            "将相关事实记入笔记，或缩小检查范围，避免重复读取同一目标。",
        ),
        "junk_reads" => (
            "读取生成文件或依赖",
            "人工排查时优先检查源码目录，并忽略生成文件与依赖目录。",
        ),
        "session_outlier" => (
            "单一会话占据大部分行为费用",
            "先检查该会话再进行全局优化；长上下文或重复重试可能仅发生在此会话。",
        ),
        _ => {
            return (
                original(&finding.title),
                original(&finding.evidence),
                original(&finding.recommendation),
            );
        }
    };
    let detail = match evidence {
        Some(E::LowReadEdit { reads, edits }) => super::current().select(
            &std::format!("{reads} read/search calls for {edits} edit calls in this filter."),
            &std::format!("当前筛选有 {reads} 次读取/搜索调用，{edits} 次编辑调用。"),
        ).to_string(),
        Some(E::DuplicateReads { session, target, calls }) => super::current().select(
            &std::format!("Session {session} read/search target `{target}` {calls} times."),
            &std::format!("会话 {session} 对目标 `{target}` 读取/搜索了 {calls} 次。"),
        ).to_string(),
        Some(E::JunkReads { calls, example }) => super::current().select(
            &std::format!("{calls} read/search calls touched generated or dependency-looking paths; example `{example}`."),
            &std::format!("{calls} 次读取/搜索涉及生成文件或依赖路径；例如 `{example}`。"),
        ).to_string(),
        Some(E::SessionOutlier { session, percent }) => super::current().select(
            &std::format!("Session {session} accounts for {percent:.1}% of turn tokens in this filter."),
            &std::format!("会话 {session} 占当前筛选轮次令牌的 {percent:.1}%。"),
        ).to_string(),
        None => original(&finding.evidence),
    };
    (
        super::text(&finding.title, pair.0).to_string(),
        detail,
        super::text(&finding.recommendation, pair.1).to_string(),
    )
}

pub(crate) fn monitor_action(probe: &crate::domain::platform_monitor::PlatformProbe) -> String {
    let zh = match probe.platform_id {
        "reasonix" => "仅监测；当前会话缺少可重放的逐轮用量",
        "gemini" => "仅监测；需要脱敏的 Gemini CLI 样本和令牌语义",
        "cursor" => "仅监测；需要稳定的本地用量导出样本",
        "copilot" => "仅监测；需要隐私审查和令牌语义",
        "zed" => "仅监测；需要 Zed 样本覆盖",
        "kiro" => "仅监测；需要 Kiro 会话样本",
        "goose" => "仅监测；需要 Goose 会话样本",
        "kimi" => "仅监测；需要 Kimi/Qwen 令牌语义",
        "roo_kilo_cline" => "仅监测；需要扩展专属样本审查",
        "codebuff" => "仅监测；需要 Codebuff 样本",
        "crush" => "仅监测；需要 Crush 数据库结构审查",
        "warp_oz" => "仅监测；需要 Warp/Oz 样本",
        "amp" => "仅监测；需要 Amp 线程样本",
        "hermes" => "仅监测；需要 Hermes 数据库结构审查",
        "trae" => "仅监测；需要 Trae 授权/隐私审查和样本",
        "openclaw_pi_droid" => "仅监测；需要各客户端样本和令牌语义",
        "gajae_code" => "仅监测；需要 Gajae-Code 样本",
        "synthetic" => "不是真实本地来源；不应导入 llmusage",
        _ => return original(probe.next_action),
    };
    super::text(probe.next_action, zh).to_string()
}
