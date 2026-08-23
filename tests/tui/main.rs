//! Property-based tests for TUI panel rendering.
//! Feature: terminal-dashboard, Properties 2, 5, 6, 7, 8, 9
//!
//! Uses `proptest` to generate random data structs and `ratatui::Terminal`
//! with `TestBackend` to render panels into a buffer, then asserts that
//! expected strings appear in the rendered output.

use proptest::prelude::*;
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
};

use llmusage::domain::platform_monitor::{ParserSupportStatus, PlatformProbe, PlatformProbeStatus};
use llmusage::query::{
    ActivityBreakdown, ActivityPayload, BehaviorSupport, CategoryCompareRow, CompareMetric,
    CompareModelCandidate, ContextPressurePayload, DailyModelPoint, DailyTrendPoint, HeatmapPoint,
    HourlyTrendPoint, ModelBreakdown, ModelComparePayload, ModelCompareStats, MonthlyTrendPoint,
    OptimizeFinding, OptimizePayload, OverviewPayload, PeriodDetailRow, SyncActionPayload,
    SyncCommandCenterPayload, SyncMetricsPayload, SyncSafetyPayload, SyncSourcePayload,
    TokenSummary, ToolBreakdown, ToolsPayload, ZombieItem, ZombieReport,
};
use llmusage::tui::app::{
    ActiveDialog, AppState, BehaviorPanelPayload, OverviewPanelPayload, Panel, PeriodDetailKind,
    PeriodDetailPayload, PeriodDetailState, ScrollState, StatsPanelPayload,
};
use llmusage::tui::format::{cost_compact, stat_compact};
use llmusage::tui::theme;

/// Extract all text content from a TestBackend buffer as a single string.
/// Handles wide (CJK) characters correctly by skipping their continuation cells.
fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buf.area.height {
        let mut x: u16 = 0;
        while x < buf.area.width {
            let cell = &buf[(x, y)];
            let sym = cell.symbol();
            if sym.is_empty() {
                x += 1;
                continue;
            }
            text.push_str(sym);
            // If this is a wide character (CJK), skip the next continuation cell
            // Wide chars in ratatui occupy 2 cells; the second cell is a space placeholder
            let char_width = sym.chars().next().map_or(1, |c| {
                if ('\u{1100}'..='\u{115F}').contains(&c)
                    || ('\u{2E80}'..='\u{A4CF}').contains(&c)
                    || ('\u{A960}'..='\u{A97F}').contains(&c)
                    || ('\u{AC00}'..='\u{D7FF}').contains(&c)
                    || ('\u{F900}'..='\u{FAFF}').contains(&c)
                    || ('\u{FE10}'..='\u{FE6F}').contains(&c)
                    || ('\u{FF01}'..='\u{FF60}').contains(&c)
                    || ('\u{FFE0}'..='\u{FFE6}').contains(&c)
                    || c > '\u{1FFFF}'
                {
                    2
                } else {
                    1
                }
            });
            x += char_width as u16;
        }
    }
    text
}

// ─── Strategies ───────────────────────────────────────────────────────────────

fn arb_model_breakdown() -> impl Strategy<Value = ModelBreakdown> {
    (
        "[a-z]{3,8}",
        1i64..20_000_000_000,
        1i64..2_000_000,
        0.0001f64..100.0,
    )
        .prop_map(|(model, tokens, events, cost)| ModelBreakdown {
            model,
            input_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: tokens,
            event_count: events,
            cost_with_cache_usd: cost,
            cost_without_cache_usd: 0.0,
            cache_savings_usd: 0.0,
            pricing_status: "static".to_string(),
            pricing_source: None,
            pricing_rate: None,
            sources: Vec::new(),
        })
}

fn sample_hourly(hour_start: &str, tokens: i64) -> HourlyTrendPoint {
    HourlyTrendPoint {
        hour_start: hour_start.to_string(),
        input_tokens: tokens,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        output_tokens: 0,
        total_tokens: tokens,
        event_count: 1,
        turn_count: 1,
        cost_with_cache_usd: 1.25,
        sources: vec!["codex".to_string()],
    }
}

fn sample_monthly(month: &str, tokens: i64, cost: f64) -> MonthlyTrendPoint {
    MonthlyTrendPoint {
        month: month.to_string(),
        input_tokens: tokens,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        output_tokens: 0,
        total_tokens: tokens,
        event_count: 1,
        turn_count: 1,
        cost_with_cache_usd: cost,
    }
}

fn render_daily_text(points: Vec<DailyTrendPoint>, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: points.len(),
        visible: height.saturating_sub(4) as usize,
    };
    let data: Option<Result<Vec<DailyTrendPoint>, String>> = Some(Ok(points));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::daily::render(frame, area, &data, &scroll);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn render_hourly_text(points: Vec<HourlyTrendPoint>, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: points.len(),
        visible: height.saturating_sub(4) as usize,
    };
    let data: Option<Result<Vec<HourlyTrendPoint>, String>> = Some(Ok(points));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::hourly::render(frame, area, &data, &scroll);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn sample_sync_payload() -> SyncCommandCenterPayload {
    SyncCommandCenterPayload {
        mode: "live".to_string(),
        tone: "good".to_string(),
        headline_key: "syncCenter.headline.ready".to_string(),
        reason_key: "syncCenter.reason.ready".to_string(),
        generated_at: "2026-06-12T00:00:00Z".to_string(),
        current_job: None,
        last_run: None,
        safety: SyncSafetyPayload {
            ordinary_sync_safe: true,
            worker_lock: "available".to_string(),
            worker_lock_holder: None,
            lossy_rebuild_risk: false,
            risk_sources: Vec::new(),
            risk_details: Vec::new(),
            recent_failures: 0,
        },
        metrics: SyncMetricsPayload {
            events_seen: 1_500,
            inserted_delta: 25,
            stored_events: 12_000,
            sources_ready: 2,
            sources_total: 3,
        },
        sources: vec![
            SyncSourcePayload {
                source: "codex".to_string(),
                status: "ok".to_string(),
                tone: "warn".to_string(),
                files_processed: 12,
                changed_files: 2,
                skipped_files: 10,
                events_seen: 1_000,
                events_inserted: 20,
                stored_events: 8_000,
                malformed_lines: 2,
                oversized_lines: 1,
                skipped_lines: 0,
                accounting_anomaly_lines: 0,
                updated_at: Some("2026-06-12T00:00:00Z".to_string()),
                share: 1.0,
                error_key: None,
                lossy_rebuild_risk: false,
            },
            SyncSourcePayload {
                source: "opencode".to_string(),
                status: "idle".to_string(),
                tone: "neutral".to_string(),
                files_processed: 4,
                changed_files: 0,
                skipped_files: 4,
                events_seen: 500,
                events_inserted: 5,
                stored_events: 4_000,
                malformed_lines: 0,
                oversized_lines: 0,
                skipped_lines: 4,
                accounting_anomaly_lines: 1,
                updated_at: Some("2026-06-11T00:00:00Z".to_string()),
                share: 0.5,
                error_key: None,
                lossy_rebuild_risk: false,
            },
        ],
        actions: vec![SyncActionPayload {
            id: "sync".to_string(),
            label_key: "syncCenter.action.sync".to_string(),
            primary: true,
            disabled: false,
            reason_key: None,
        }],
    }
}

fn sample_platform_probes() -> Vec<PlatformProbe> {
    vec![
        PlatformProbe {
            platform_id: "codex",
            display_name: "Codex",
            source_kind: Some(llmusage::SourceKind::Codex),
            status: PlatformProbeStatus::Detected,
            parser_status: ParserSupportStatus::Registered,
            quality: Some("precise"),
            privacy: "local_artifacts",
            roots_checked: 1,
            roots_detected: 1,
            artifact_patterns: &["*.jsonl"],
            detail: "candidate roots detected".to_string(),
            next_action: "parsed by the registered Codex source parser",
        },
        PlatformProbe {
            platform_id: "gemini",
            display_name: "Gemini CLI",
            source_kind: None,
            status: PlatformProbeStatus::Detected,
            parser_status: ParserSupportStatus::BlockedNoSamples,
            quality: None,
            privacy: "local_artifacts",
            roots_checked: 1,
            roots_detected: 1,
            artifact_patterns: &["*.jsonl"],
            detail: "candidate roots detected".to_string(),
            next_action: "monitor-only; requires sanitized Gemini CLI samples",
        },
    ]
}

fn render_sync_status_text(
    payload: SyncCommandCenterPayload,
    probes: Vec<PlatformProbe>,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: payload.sources.len() + probes.len(),
        visible: height.saturating_sub(8) as usize,
    };
    let data: Option<Result<SyncCommandCenterPayload, String>> = Some(Ok(payload));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::sync_status::render(frame, area, &data, &probes, &scroll);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn sample_quota_report() -> llmusage::subscription::UsageFetchReport {
    use llmusage::subscription::{
        UsageFetchDiagnostic, UsageFetchReport, UsageMetric, UsageOutput,
    };
    UsageFetchReport {
        outputs: vec![UsageOutput {
            provider: "Grok Build".into(),
            account: None,
            credential_source: None,
            plan: Some("Unknown".into()),
            email: Some("user@example.com".into()),
            metrics: vec![UsageMetric {
                label: "Weekly".into(),
                used_percent: 30.0,
                remaining_percent: 70.0,
                remaining_label: Some("70% left".into()),
                resets_at: Some("2026-08-24T00:18:00Z".into()),
            }],
        }],
        diagnostics: vec![UsageFetchDiagnostic::error(
            "Claude",
            "Claude usage request failed (HTTP 429 Too Many Requests)",
        )],
    }
}

fn render_usage_quota_text(
    report: llmusage::subscription::UsageFetchReport,
    hide_emails: bool,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: report.outputs.len(),
        visible: height.saturating_sub(8) as usize,
    };

    terminal
        .draw(|frame| {
            llmusage::tui::panels::usage::render(
                frame,
                area,
                &Some(report),
                false,
                hide_emails,
                &scroll,
            );
        })
        .unwrap();

    buffer_text(&terminal)
}

fn render_overview_text(payload: OverviewPanelPayload, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let data: Option<Result<OverviewPanelPayload, String>> = Some(Ok(payload));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::overview::render(frame, area, &data);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn sample_overview_totals() -> OverviewPayload {
    OverviewPayload {
        generated_at: "2026-06-12T00:00:00Z".to_string(),
        total: TokenSummary {
            input_tokens: 10_000,
            cache_creation_tokens: 500,
            cache_read_tokens: 2_500,
            output_tokens: 8_000,
            reasoning_output_tokens: 1_000,
            total_tokens: 22_000,
        },
        last_24h: TokenSummary::default(),
        source_count: 2,
        bucket_count: 10,
        total_events: 12,
        last_24h_events: 2,
        total_cost_usd: 3.5,
        cache_efficiency: 0.25,
        last_sync_at: Some("2026-06-12T00:00:00Z".to_string()),
        last_export_at: None,
    }
}

fn sample_overview_model(
    model: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    cost: f64,
) -> ModelBreakdown {
    ModelBreakdown {
        model: model.to_string(),
        input_tokens: input,
        cache_creation_tokens: cache_write,
        cache_read_tokens: cache_read,
        output_tokens: output,
        reasoning_output_tokens: 0,
        total_tokens: input + output + cache_read + cache_write,
        event_count: 1,
        cost_with_cache_usd: cost,
        cost_without_cache_usd: cost,
        cache_savings_usd: 0.0,
        pricing_status: "static".to_string(),
        pricing_source: None,
        pricing_rate: None,
        sources: Vec::new(),
    }
}

fn sample_overview_payload() -> OverviewPanelPayload {
    OverviewPanelPayload {
        totals: sample_overview_totals(),
        daily_models: vec![
            DailyModelPoint {
                date: "2026-07-18".to_string(),
                model: "gpt-5.5".to_string(),
                total_tokens: 1_000,
            },
            DailyModelPoint {
                date: "2026-07-19".to_string(),
                model: "claude-opus-5".to_string(),
                total_tokens: 2_000,
            },
        ],
        models: vec![
            sample_overview_model(
                "gpt-5.5",
                640_400_000,
                41_700_000,
                6_300_000_000,
                0,
                8_200.0,
            ),
            sample_overview_model(
                "claude-opus-5",
                11_500_000,
                4_100_000,
                874_000_000,
                107_200_000,
                1_260.0,
            ),
        ],
    }
}

fn sample_stats_payload() -> StatsPanelPayload {
    StatsPanelPayload {
        overview: sample_overview_totals(),
        heatmap: vec![
            HeatmapPoint {
                date: "2026-06-09".to_string(),
                event_count: 0,
                total_tokens: 0,
            },
            HeatmapPoint {
                date: "2026-06-10".to_string(),
                event_count: 1,
                total_tokens: 4_000,
            },
            HeatmapPoint {
                date: "2026-06-11".to_string(),
                event_count: 3,
                total_tokens: 8_000,
            },
            HeatmapPoint {
                date: "2026-06-12".to_string(),
                event_count: 2,
                total_tokens: 6_000,
            },
        ],
        models: vec![sample_overview_model(
            "gpt-5.5", 10_000, 8_000, 2_500, 500, 3.5,
        )],
        context_pressure: ContextPressurePayload {
            peak_percent: 0.42,
            avg_percent: 0.18,
            peak_model: Some("codex:gpt-5".to_string()),
            priced_events: 12,
            unpriced_events: 0,
        },
    }
}

fn render_stats_text(payload: StatsPanelPayload, width: u16, height: u16) -> String {
    render_stats_text_with_detail(payload, None, width, height)
}

fn render_stats_text_with_detail(
    payload: StatsPanelPayload,
    detail: Option<&PeriodDetailState>,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: 0,
        visible: height.saturating_sub(10) as usize,
    };
    let data: Option<Result<StatsPanelPayload, String>> = Some(Ok(payload));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::stats::render(frame, area, &data, &scroll, detail);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn support(supported: bool, level: &str, reason: Option<&str>) -> BehaviorSupport {
    BehaviorSupport {
        supported,
        level: level.to_string(),
        reason: reason.map(str::to_string),
    }
}

fn compare_stats(model: &str, calls: i64, edit_turns: i64, cost: f64) -> ModelCompareStats {
    ModelCompareStats {
        model: model.to_string(),
        calls,
        turns: calls / 2,
        edit_turns,
        one_shot_turns: edit_turns / 2,
        retries: calls / 10,
        total_tokens: calls * 1_000,
        estimated_cost_usd: cost,
        cache_efficiency: 0.42,
        cost_per_call: cost / calls as f64,
        cost_per_edit_turn: cost / edit_turns.max(1) as f64,
        one_shot_rate: 0.5,
        retry_rate: 0.1,
        avg_tools_per_turn: 2.0,
        delegation_rate: 0.2,
        planning_rate: 0.3,
        low_sample: false,
    }
}

fn sample_behavior_payload() -> BehaviorPanelPayload {
    BehaviorPanelPayload {
        activity: ActivityPayload {
            support: support(true, "normalized", None),
            breakdown: vec![ActivityBreakdown {
                category: "coding".to_string(),
                turns: 12,
                edit_turns: 8,
                one_shot_turns: 5,
                retries: 2,
                call_count: 14,
                total_tokens: 42_000,
                estimated_cost_usd: 1.25,
                one_shot_rate: 0.625,
                retry_rate: 0.166,
            }],
        },
        tools: ToolsPayload {
            support: support(true, "normalized", None),
            breakdown: vec![
                ToolBreakdown {
                    tool_kind: "read".to_string(),
                    tool_name: "Read".to_string(),
                    mcp_server: Some("filesystem".to_string()),
                    calls: 7,
                    turn_count: 4,
                    session_count: 2,
                    estimated_cost_usd: 0.42,
                    call_share: 0.35,
                    first_seen_at: Some("2026-05-17T00:00:00Z".to_string()),
                    last_seen_at: Some("2026-05-17T01:00:00Z".to_string()),
                },
                ToolBreakdown {
                    tool_kind: "(non-tool)".to_string(),
                    tool_name: "(non-tool)".to_string(),
                    mcp_server: None,
                    calls: 0,
                    turn_count: 3,
                    session_count: 1,
                    estimated_cost_usd: 0.17,
                    call_share: 0.0,
                    first_seen_at: Some("2026-05-17T00:30:00Z".to_string()),
                    last_seen_at: Some("2026-05-17T01:30:00Z".to_string()),
                },
            ],
        },
        optimize: OptimizePayload {
            support: support(true, "normalized", None),
            score: 72,
            grade: "C".to_string(),
            estimated_savings_tokens: 8_000,
            estimated_savings_usd: 0.8,
            findings: vec![OptimizeFinding {
                id: "duplicate_reads".to_string(),
                title: "Repeated reads".to_string(),
                severity: "medium".to_string(),
                evidence: "Read called repeatedly for same path".to_string(),
                recommendation: "Cache context before re-reading".to_string(),
                estimated_savings_tokens: 8_000,
                estimated_savings_usd: 0.8,
            }],
        },
        zombie: ZombieReport {
            installed_total: 3,
            zombies: vec![ZombieItem {
                source: "claude".to_string(),
                kind: "skill".to_string(),
                name: "smart-search".to_string(),
            }],
        },
        compare: ModelComparePayload {
            support: support(true, "normalized", None),
            candidates: vec![
                CompareModelCandidate {
                    model: "gpt-5".to_string(),
                    calls: 80,
                    turns: 40,
                    edit_turns: 30,
                    total_tokens: 80_000,
                    estimated_cost_usd: 5.5,
                    low_sample: false,
                },
                CompareModelCandidate {
                    model: "sonnet".to_string(),
                    calls: 70,
                    turns: 35,
                    edit_turns: 25,
                    total_tokens: 70_000,
                    estimated_cost_usd: 4.5,
                    low_sample: false,
                },
            ],
            model_a: Some(compare_stats("gpt-5", 80, 30, 5.5)),
            model_b: Some(compare_stats("sonnet", 70, 25, 4.5)),
            metrics: vec![CompareMetric {
                id: "one_shot_rate".to_string(),
                label: "One-shot rate".to_string(),
                model_a_value: 0.5,
                model_b_value: 0.44,
                higher_is_better: true,
            }],
            category_head_to_head: vec![CategoryCompareRow {
                category: "coding".to_string(),
                model_a_edit_turns: 30,
                model_a_one_shot_rate: 0.5,
                model_b_edit_turns: 25,
                model_b_one_shot_rate: 0.44,
            }],
            working_style: Vec::new(),
            warning: None,
        },
    }
}

fn render_behavior_text(payload: BehaviorPanelPayload, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let data: Option<Result<BehaviorPanelPayload, String>> = Some(Ok(payload));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::behavior::render(frame, area, &data);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn render_nav_text(active_panel: Panel, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);

    terminal
        .draw(|frame| {
            llmusage::tui::nav_bar::render(frame, area, active_panel);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn render_shell_text(mut state: AppState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    state.handle_resize(width, height);

    terminal
        .draw(|frame| {
            llmusage::tui::draw::draw(frame, &state);
        })
        .unwrap();

    buffer_text(&terminal)
}

// ─── Property Tests ───────────────────────────────────────────────────────────

// Feature: terminal-dashboard, Property 2: Overview panel renders all required fields
// **Validates: Requirements 3.1, 3.2**
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_overview_panel_renders_all_required_fields(model in arb_model_breakdown()) {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let area = Rect::new(0, 0, 120, 30);
        let payload = OverviewPanelPayload {
            totals: sample_overview_totals(),
            daily_models: vec![DailyModelPoint {
                date: "2026-07-18".to_string(),
                model: model.model.clone(),
                total_tokens: model.total_tokens,
            }],
            models: vec![model.clone()],
        };
        let data: Option<Result<OverviewPanelPayload, String>> = Some(Ok(payload));

        terminal.draw(|frame| {
            llmusage::tui::panels::overview::render(frame, area, &data);
        }).unwrap();

        let text = buffer_text(&terminal);
        prop_assert!(text.contains("Tokens per Day"), "missing chart title: {text}");
        prop_assert!(text.contains("Models by Cost"), "missing list title: {text}");
        prop_assert!(text.contains(&model.model), "missing model name: {text}");
        let input = stat_compact(model.input_tokens);
        prop_assert!(text.contains(&input), "missing input '{input}': {text}");
    }
}

// Feature: terminal-dashboard, Property 5: Model table renders all required columns
// **Validates: Requirements 5.1**
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_model_table_renders_all_required_columns(
        items in proptest::collection::vec(arb_model_breakdown(), 1..4)
    ) {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let area = Rect::new(0, 0, 120, 30);
        let scroll = ScrollState { offset: 0, selected: 0, total: items.len(), visible: 25 };
        let data: Option<Result<Vec<ModelBreakdown>, String>> = Some(Ok(items.clone()));

        terminal.draw(|frame| {
            llmusage::tui::panels::models::render(frame, area, &data, &scroll);
        }).unwrap();

        let text = buffer_text(&terminal);

        for item in &items {
            prop_assert!(text.contains(&item.model),
                "Missing model name '{}' in output", item.model);
            let tokens_str = stat_compact(item.total_tokens);
            prop_assert!(text.contains(&tokens_str),
                "Missing total_tokens '{}' in output", tokens_str);
            let events_str = stat_compact(item.event_count);
            prop_assert!(text.contains(&events_str),
                "Missing event_count '{}' in output", events_str);
            let cost_str = cost_compact(item.cost_with_cache_usd);
            prop_assert!(text.contains(&cost_str),
                "Missing cost_with_cache_usd '{}' in output", cost_str);
        }
    }
}

fn sample_model(model: &str, tokens: i64, cost: f64) -> ModelBreakdown {
    ModelBreakdown {
        model: model.to_string(),
        input_tokens: tokens / 2,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
        output_tokens: tokens / 2,
        reasoning_output_tokens: 0,
        total_tokens: tokens,
        event_count: 1,
        cost_with_cache_usd: cost,
        cost_without_cache_usd: cost,
        cache_savings_usd: 0.0,
        pricing_status: "static".to_string(),
        pricing_source: None,
        pricing_rate: None,
        sources: vec!["codex".to_string()],
    }
}

fn render_models_text(items: Vec<ModelBreakdown>, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: items.len(),
        visible: height.saturating_sub(4) as usize,
    };
    let data: Option<Result<Vec<ModelBreakdown>, String>> = Some(Ok(items));
    terminal
        .draw(|frame| {
            llmusage::tui::panels::models::render(frame, area, &data, &scroll);
        })
        .unwrap();
    buffer_text(&terminal)
}

mod panels;
mod shell;
