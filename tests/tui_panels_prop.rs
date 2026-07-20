//! Property-based tests for TUI panel rendering.
//! Feature: terminal-dashboard, Properties 2, 5, 6, 7, 8, 9
//!
//! Uses `proptest` to generate random data structs and `ratatui::Terminal`
//! with `TestBackend` to render panels into a buffer, then asserts that
//! expected strings appear in the rendered output.

use proptest::prelude::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use llmusage::domain::platform_monitor::{ParserSupportStatus, PlatformProbe, PlatformProbeStatus};
use llmusage::query::{
    ActivityBreakdown, ActivityPayload, BehaviorSupport, CategoryCompareRow, CompareMetric,
    CompareModelCandidate, ContextPressurePayload, CostLine, CursorHealth, DailyTrendPoint,
    HealthPayload, HeatmapPoint, ModelBreakdown, ModelComparePayload, ModelCompareStats,
    OptimizeFinding, OptimizePayload, OverviewPayload, ProjectBreakdown, SourceBreakdown,
    SyncActionPayload, SyncCommandCenterPayload, SyncMetricsPayload, SyncSafetyPayload,
    SyncSourcePayload, TokenSummary, ToolBreakdown, ToolsPayload, TrendPoint, ZombieItem,
    ZombieReport,
};
use llmusage::store::{IntegrationState, RunRecord};
use llmusage::tui::app::{
    ActiveDialog, AppState, BehaviorPanelPayload, Panel, ScrollState, StatsPanelPayload, TimeWindow,
};

/// Format a number with thousands separators (matching panel rendering).
fn format_number(n: i64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    if n < 0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

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

fn arb_token_summary() -> impl Strategy<Value = TokenSummary> {
    (
        0i64..100_000,
        0i64..100_000,
        0i64..100_000,
        0i64..100_000,
        0i64..100_000,
    )
        .prop_map(
            |(input, cache_creation, cache, output, reasoning)| TokenSummary {
                input_tokens: input,
                cache_creation_tokens: cache_creation,
                cache_read_tokens: cache,
                output_tokens: output,
                reasoning_output_tokens: reasoning,
                total_tokens: input + cache_creation + cache + output + reasoning,
            },
        )
}

fn arb_overview_payload() -> impl Strategy<Value = OverviewPayload> {
    (
        arb_token_summary(),
        arb_token_summary(),
        0i64..100,
        0i64..1000,
        0.0f64..10000.0,
        0.0f64..1.0,
        proptest::option::of("[a-z]{5,10}"),
    )
        .prop_map(
            |(total, last_24h, source_count, bucket_count, cost, efficiency, last_sync)| {
                OverviewPayload {
                    generated_at: "2025-01-01T00:00:00Z".to_string(),
                    total,
                    last_24h,
                    source_count,
                    bucket_count,
                    total_events: 0,
                    last_24h_events: 0,
                    total_cost_usd: cost,
                    cache_efficiency: efficiency,
                    last_sync_at: last_sync,
                    last_export_at: None,
                }
            },
        )
}

fn arb_model_breakdown() -> impl Strategy<Value = ModelBreakdown> {
    ("[a-z]{3,8}", 1i64..100_000, 1i64..1000, 0.0001f64..100.0).prop_map(
        |(model, tokens, events, cost)| ModelBreakdown {
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
        },
    )
}

fn arb_source_breakdown() -> impl Strategy<Value = SourceBreakdown> {
    (
        "[a-z]{3,8}",
        1i64..100_000,
        1i64..1000,
        proptest::option::of("[0-9]{4}-[0-9]{2}-[0-9]{2}"),
    )
        .prop_map(|(source, tokens, events, last_event)| SourceBreakdown {
            source,
            total_tokens: tokens,
            event_count: events,
            last_event_at: last_event,
        })
}

fn arb_project_breakdown() -> impl Strategy<Value = ProjectBreakdown> {
    ("[a-z]{3,10}", 1i64..100_000, 1i64..1000, 0.0001f64..100.0).prop_map(
        |(label, tokens, events, cost)| ProjectBreakdown {
            project_hash: "hash".to_string(),
            project_label: label,
            project_ref: None,
            total_tokens: tokens,
            event_count: events,
            total_cost_usd: cost,
            project_path: None,
        },
    )
}

fn arb_cost_line() -> impl Strategy<Value = CostLine> {
    (
        "[a-z]{3,8}",
        "[a-z]{3,8}",
        1i64..100_000,
        0.01f64..100.0,
        1i64..1000,
    )
        .prop_map(|(source, model, tokens, cost, events)| CostLine {
            source,
            model,
            total_tokens: tokens,
            estimated_cost_usd: cost,
            event_count: events,
        })
}

fn arb_integration_state() -> impl Strategy<Value = IntegrationState> {
    ("[a-z]{3,8}", "[a-z]{4,8}").prop_map(|(source, status)| IntegrationState {
        source,
        install_type: "init".to_string(),
        status,
        config_path: None,
        backup_path: None,
        details_json: None,
        updated_at: "2025-01-01T00:00:00Z".to_string(),
    })
}

fn arb_cursor_health() -> impl Strategy<Value = CursorHealth> {
    ("[a-z]{3,8}", "[a-z]{3,10}").prop_map(|(source, key)| CursorHealth {
        source,
        cursor_key: key,
        updated_at: Some("2025-01-01T00:00:00Z".to_string()),
        sqlite_status: None,
    })
}

fn arb_run_record() -> impl Strategy<Value = RunRecord> {
    ("[a-z]{3,8}", proptest::option::of("[a-z ]{5,15}")).prop_map(|(cmd, error)| RunRecord {
        id: 1,
        command: cmd,
        status: "failed".to_string(),
        summary: None,
        error,
        started_at: "2025-01-01T00:00:00Z".to_string(),
        finished_at: None,
    })
}

fn arb_health_payload() -> impl Strategy<Value = HealthPayload> {
    (
        proptest::collection::vec(arb_integration_state(), 0..5),
        proptest::collection::vec(arb_cursor_health(), 0..5),
        proptest::collection::vec(arb_run_record(), 0..15),
    )
        .prop_map(|(integrations, cursors, recent_failures)| HealthPayload {
            integrations,
            cursors,
            recent_failures,
        })
}

fn arb_trend_points() -> impl Strategy<Value = Vec<TrendPoint>> {
    proptest::collection::vec(
        ("2026-05-(0[1-9]|1[0-9]|2[0-8])", 0i64..1_000_000).prop_map(|(label, tokens)| {
            TrendPoint {
                label,
                total_tokens: tokens,
            }
        }),
        0..35,
    )
}

fn render_trends_text(
    points: Vec<TrendPoint>,
    window: TimeWindow,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let data: Option<Result<Vec<TrendPoint>, String>> = Some(Ok(points));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::trends::render(frame, area, &data, window);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn render_daily_text(points: Vec<DailyTrendPoint>, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
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

fn render_hourly_text(points: Vec<TrendPoint>, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        total: points.len(),
        visible: height.saturating_sub(4) as usize,
    };
    let data: Option<Result<Vec<TrendPoint>, String>> = Some(Ok(points));

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
                tone: "good".to_string(),
                files_processed: 12,
                changed_files: 2,
                skipped_files: 10,
                events_seen: 1_000,
                events_inserted: 20,
                stored_events: 8_000,
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

fn render_usage_text(
    payload: SyncCommandCenterPayload,
    probes: Vec<PlatformProbe>,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        total: payload.sources.len() + probes.len(),
        visible: height.saturating_sub(8) as usize,
    };
    let data: Option<Result<SyncCommandCenterPayload, String>> = Some(Ok(payload));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::usage::render(frame, area, &data, &probes, &scroll);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn render_overview_text(payload: OverviewPayload, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let data: Option<Result<OverviewPayload, String>> = Some(Ok(payload));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::overview::render(frame, area, &data);
        })
        .unwrap();

    buffer_text(&terminal)
}

fn sample_overview_payload() -> OverviewPayload {
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

#[test]
fn overview_panel_renders_summary_sections_and_24h_pulse() {
    let mut payload = sample_overview_payload();
    payload.last_24h = TokenSummary {
        input_tokens: 1_000,
        cache_creation_tokens: 500,
        cache_read_tokens: 1_000,
        output_tokens: 2_000,
        reasoning_output_tokens: 1_000,
        total_tokens: 5_500,
    };
    payload.last_24h_events = 2;

    let text = render_overview_text(payload, 120, 30);

    for expected in [
        "Token Mix",
        "Recent Activity",
        "Freshness",
        "24h Pulse",
        "Input",
        "Cache read",
        "Avg/event",
        "Generated",
        "All-time share",
        "5,500",
        "2,750",
        "25.0%",
    ] {
        assert!(
            text.contains(expected),
            "overview panel should contain '{expected}', got: {text}"
        );
    }
}

fn sample_stats_payload() -> StatsPanelPayload {
    StatsPanelPayload {
        overview: sample_overview_payload(),
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
        sources: vec![
            SourceBreakdown {
                source: "codex".to_string(),
                total_tokens: 16_000,
                last_event_at: Some("2026-06-12T00:00:00Z".to_string()),
                event_count: 8,
            },
            SourceBreakdown {
                source: "opencode".to_string(),
                total_tokens: 6_000,
                last_event_at: None,
                event_count: 4,
            },
        ],
        health: HealthPayload {
            integrations: vec![IntegrationState {
                source: "codex".to_string(),
                install_type: "init".to_string(),
                status: "ok".to_string(),
                config_path: None,
                backup_path: None,
                details_json: None,
                updated_at: "2026-06-12T00:00:00Z".to_string(),
            }],
            cursors: vec![CursorHealth {
                source: "codex".to_string(),
                cursor_key: "session".to_string(),
                updated_at: Some("2026-06-12T00:00:00Z".to_string()),
                sqlite_status: None,
            }],
            recent_failures: Vec::new(),
        },
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
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let area = Rect::new(0, 0, width, height);
    let scroll = ScrollState {
        offset: 0,
        total: payload.sources.len(),
        visible: height.saturating_sub(10) as usize,
    };
    let data: Option<Result<StatsPanelPayload, String>> = Some(Ok(payload));

    terminal
        .draw(|frame| {
            llmusage::tui::panels::stats::render(frame, area, &data, &scroll);
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

#[test]
fn nav_bar_renders_agents_panel_shortcut() {
    let text = render_nav_text(Panel::Behavior, 120, 3);
    assert!(
        text.contains("8 Agents"),
        "nav bar should expose behavior panel as 8 Agents, got: {text}"
    );
}

#[test]
fn dashboard_shell_renders_tokscale_style_header_and_footer() {
    let text = render_shell_text(AppState::new(), 120, 30);

    for expected in [
        "llmusage",
        "Overview",
        "Usage",
        "Daily",
        "Hourly",
        "[s:source]",
        "[r:refresh]",
        "[x:sync]",
        "[?]",
    ] {
        assert!(
            text.contains(expected),
            "dashboard shell should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn dashboard_shell_uses_short_labels_on_narrow_widths() {
    let text = render_shell_text(AppState::new(), 50, 18);

    for expected in ["llmusage", "Ovw", "Use", "Day", "Hr", "tab/1-9"] {
        assert!(
            text.contains(expected),
            "narrow dashboard shell should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn source_picker_overlay_lists_monitor_only_platforms() {
    let mut state = AppState::new();
    state.active_dialog = Some(ActiveDialog::SourcePicker);

    let text = render_shell_text(state, 120, 30);

    for expected in ["Sources", "Gemini CLI", "blocked_no_samples"] {
        assert!(
            text.contains(expected),
            "source picker should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn daily_panel_renders_tokscale_style_token_channels() {
    let text = render_daily_text(
        vec![
            DailyTrendPoint {
                date: "2026-05-28".to_string(),
                input_tokens: 1_000,
                cache_read_tokens: 2_000,
                cache_creation_tokens: 500,
                output_tokens: 3_000,
                total_tokens: 6_500,
                event_count: 7,
                cost_with_cache_usd: 1.25,
            },
            DailyTrendPoint {
                date: "2026-05-29".to_string(),
                input_tokens: 2_000,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 4_000,
                total_tokens: 6_000,
                event_count: 3,
                cost_with_cache_usd: 2.5,
            },
        ],
        120,
        16,
    );

    for expected in [
        "Daily Usage",
        "2026-05-29",
        "Input",
        "Output",
        "Cache R",
        "Cache W",
        "detail",
        "$2.50",
    ] {
        assert!(
            text.contains(expected),
            "daily panel should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn daily_panel_uses_compact_columns_on_narrow_widths() {
    let text = render_daily_text(
        vec![DailyTrendPoint {
            date: "2026-05-29".to_string(),
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 2,
            total_tokens: 3,
            event_count: 1,
            cost_with_cache_usd: 0.01,
        }],
        50,
        10,
    );

    assert!(text.contains("Daily Usage"), "panel title missing: {text}");
    assert!(text.contains("05-29"), "compact date missing: {text}");
    assert!(
        !text.contains("2026-05-29"),
        "narrow daily panel should not keep full date: {text}"
    );
}

#[test]
fn hourly_panel_renders_profile_bars_and_compact_hour_labels() {
    let text = render_hourly_text(
        vec![
            TrendPoint {
                label: "2026-05-29T13:00:00Z".to_string(),
                total_tokens: 1_000,
            },
            TrendPoint {
                label: "2026-05-29T14:00:00Z".to_string(),
                total_tokens: 2_000,
            },
        ],
        120,
        12,
    );

    for expected in ["Hourly Usage", "05-29 14:00", "Profile", "#"] {
        assert!(
            text.contains(expected),
            "hourly panel should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn usage_panel_renders_sync_status_and_platform_monitor_summary() {
    let text = render_usage_text(sample_sync_payload(), sample_platform_probes(), 120, 18);

    for expected in [
        "Usage / Sync",
        "Ready to sync",
        "Source Sync",
        "Skipped",
        "codex",
        "8,000",
        "Platform Monitor",
        "Gemini CLI",
        "blocked-no-samples",
    ] {
        assert!(
            text.contains(expected),
            "usage panel should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn usage_panel_uses_compact_columns_on_narrow_widths() {
    let text = render_usage_text(sample_sync_payload(), sample_platform_probes(), 52, 12);

    for expected in ["Usage / Sync", "Stored", "codex", "8,000"] {
        assert!(
            text.contains(expected),
            "narrow usage panel should contain '{expected}', got: {text}"
        );
    }
    assert!(
        !text.contains("Inserted"),
        "narrow usage panel should hide wide-only columns: {text}"
    );
}

#[test]
fn stats_panel_renders_contribution_and_source_mix() {
    let text = render_stats_text(sample_stats_payload(), 120, 20);

    for expected in [
        "Stats",
        "Contribution",
        "Source Mix",
        "current streak",
        "3.50",
        "codex",
        "16.0k",
        "Health Signals",
    ] {
        assert!(
            text.contains(expected),
            "stats panel should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn stats_panel_uses_compact_source_columns_on_narrow_widths() {
    let text = render_stats_text(sample_stats_payload(), 50, 12);

    for expected in ["Stats", "Source Mix", "codex", "16.0k"] {
        assert!(
            text.contains(expected),
            "narrow stats panel should contain '{expected}', got: {text}"
        );
    }
    assert!(
        !text.contains("Last Event"),
        "narrow stats panel should hide wide-only columns: {text}"
    );
}

#[test]
fn trends_week_labels_are_short_dates_not_truncated_years() {
    let points = (10..=16)
        .map(|day| TrendPoint {
            label: format!("2026-05-{day:02}"),
            total_tokens: i64::from(day) * 1_000,
        })
        .collect();

    let text = render_trends_text(points, TimeWindow::Week7d, 120, 28);

    assert!(
        text.contains("05-16"),
        "expected short MM-DD label in trends output: {text}"
    );
    assert!(
        !text.contains("2026-05-16"),
        "full date labels should not leak into the TUI chart/table: {text}"
    );
    assert!(
        !text.contains("202 202"),
        "truncated repeated year labels should not appear: {text}"
    );
}

#[test]
fn trends_window_labels_use_window_specific_short_formats() {
    let cases = [
        (
            TimeWindow::Day24h,
            TrendPoint {
                label: "2026-05-16T09:30:00Z".to_string(),
                total_tokens: 12_000,
            },
            "09:30",
        ),
        (
            TimeWindow::Week7d,
            TrendPoint {
                label: "2026-05-16".to_string(),
                total_tokens: 12_000,
            },
            "05-16",
        ),
        (
            TimeWindow::Month30d,
            TrendPoint {
                label: "2026-05-16".to_string(),
                total_tokens: 12_000,
            },
            "05-16",
        ),
        (
            TimeWindow::All,
            TrendPoint {
                label: "2026-05".to_string(),
                total_tokens: 12_000,
            },
            "2026-05",
        ),
    ];

    for (window, point, expected_label) in cases {
        let text = render_trends_text(vec![point], window, 96, 24);
        assert!(
            text.contains(expected_label),
            "expected {expected_label} for {window:?}, got: {text}"
        );
    }
}

#[test]
fn trends_empty_data_renders_placeholder() {
    let text = render_trends_text(Vec::new(), TimeWindow::Week7d, 80, 18);
    assert!(
        text.contains("No trend data found."),
        "empty trend data should show placeholder: {text}"
    );
}

#[test]
fn trends_small_terminal_does_not_panic() {
    let points = (1..=30)
        .map(|day| TrendPoint {
            label: format!("2026-05-{day:02}"),
            total_tokens: i64::from(day) * 100,
        })
        .collect();

    let text = render_trends_text(points, TimeWindow::Month30d, 30, 8);
    assert!(
        text.contains("Usage Trends"),
        "small terminal should render a shell"
    );
}

#[test]
fn behavior_panel_renders_all_behavior_sections_and_sample_rows() {
    let text = render_behavior_text(sample_behavior_payload(), 160, 40);

    for expected in [
        "Behavior",
        "Activity",
        "Tools",
        "Optimize",
        "Model Comparison",
        "coding",
        "Read",
        "(non-tool)",
        "gpt-5",
        "sonnet",
        "Repeated reads",
    ] {
        assert!(
            text.contains(expected),
            "expected behavior panel to contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn behavior_panel_renders_no_data_degraded_and_compare_warnings() {
    let mut payload = sample_behavior_payload();
    payload.activity = ActivityPayload {
        support: support(
            false,
            "no_data",
            Some("No normalized behavior facts match this filter."),
        ),
        breakdown: Vec::new(),
    };
    payload.tools = ToolsPayload {
        support: support(false, "degraded", Some("Tool-level evidence unavailable.")),
        breakdown: Vec::new(),
    };
    payload.optimize = OptimizePayload {
        support: support(
            false,
            "no_data",
            Some("No behavior facts for optimization."),
        ),
        score: 100,
        grade: "A".to_string(),
        estimated_savings_tokens: 0,
        estimated_savings_usd: 0.0,
        findings: Vec::new(),
    };
    payload.compare = ModelComparePayload {
        support: support(
            false,
            "insufficient_models",
            Some("At least two models with local usage are required for comparison."),
        ),
        candidates: vec![CompareModelCandidate {
            model: "gpt-5".to_string(),
            calls: 1,
            turns: 1,
            edit_turns: 0,
            total_tokens: 1_000,
            estimated_cost_usd: 0.05,
            low_sample: true,
        }],
        model_a: None,
        model_b: None,
        metrics: Vec::new(),
        category_head_to_head: Vec::new(),
        working_style: Vec::new(),
        warning: Some("Need at least two models in the current filter.".to_string()),
    };

    let text = render_behavior_text(payload, 160, 40);

    for expected in [
        "no-data",
        "degraded",
        "insufficient-models",
        "score and savings are not calculated",
        "At least two",
    ] {
        assert!(
            text.contains(expected),
            "expected degraded behavior panel to contain '{expected}', got: {text}"
        );
    }
    assert!(
        !text.contains("Score 100"),
        "unsupported optimize state must not present no-data as a perfect score: {text}"
    );
}

// Feature: terminal-dashboard, Property 2: Overview panel renders all required fields
// **Validates: Requirements 3.1, 3.2**
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_overview_panel_renders_all_required_fields(payload in arb_overview_payload()) {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let area = Rect::new(0, 0, 120, 30);
        let data: Option<Result<OverviewPayload, String>> = Some(Ok(payload.clone()));

        terminal.draw(|frame| {
            llmusage::tui::panels::overview::render(frame, area, &data);
        }).unwrap();

        let text = buffer_text(&terminal);

        // total_cost_usd with 2 decimal places
        let cost_str = format!("{:.2}", payload.total_cost_usd);
        prop_assert!(text.contains(&cost_str),
            "Missing total_cost_usd '{}' in output", cost_str);

        // cache_efficiency as percentage (1 decimal place)
        let eff_str = format!("{:.1}%", payload.cache_efficiency * 100.0);
        prop_assert!(text.contains(&eff_str),
            "Missing cache_efficiency '{}' in output", eff_str);

        // source_count
        let sc_str = payload.source_count.to_string();
        prop_assert!(text.contains(&sc_str),
            "Missing source_count '{}' in output", sc_str);

        // bucket_count
        let bc_str = payload.bucket_count.to_string();
        prop_assert!(text.contains(&bc_str),
            "Missing bucket_count '{}' in output", bc_str);

        // last_sync_at or "Never synced"
        match &payload.last_sync_at {
            Some(ts) => prop_assert!(text.contains(ts),
                "Missing last_sync_at '{}' in output", ts),
            None => prop_assert!(text.contains("Never synced"),
                "Missing 'Never synced' placeholder in output"),
        }
    }
}

#[test]
fn models_visible_window_matches_full_dataset_buffer() {
    let items: Vec<ModelBreakdown> = (0..40)
        .map(|index| ModelBreakdown {
            model: format!("model-{index:02}"),
            input_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 10_000,
            event_count: 100,
            cost_with_cache_usd: 1.25,
            cost_without_cache_usd: 0.0,
            cache_savings_usd: 0.0,
            pricing_status: "static".to_string(),
            pricing_source: None,
            pricing_rate: None,
        })
        .collect();
    let visible = 7usize;
    let scroll = ScrollState {
        offset: 0,
        total: items.len(),
        visible,
    };
    let area = Rect::new(0, 0, 120, (visible + 4) as u16);

    let render = |rows: Vec<ModelBreakdown>| {
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        let data = Some(Ok(rows));
        terminal
            .draw(|frame| llmusage::tui::panels::models::render(frame, area, &data, &scroll))
            .unwrap();
        terminal.backend().buffer().clone()
    };

    assert_eq!(render(items.clone()), render(items[..visible].to_vec()));
}

#[test]
fn cost_visible_window_matches_full_dataset_buffer() {
    let items: Vec<CostLine> = (0..40)
        .map(|index| CostLine {
            source: "codex".to_string(),
            model: format!("model-{index:02}"),
            total_tokens: 10_000,
            estimated_cost_usd: 1.25,
            event_count: 100,
        })
        .collect();
    let visible = 7usize;
    let scroll = ScrollState {
        offset: 0,
        total: items.len(),
        visible,
    };
    let area = Rect::new(0, 0, 120, (visible + 4) as u16);

    let render = |rows: Vec<CostLine>| {
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        let data = Some(Ok(rows));
        terminal
            .draw(|frame| llmusage::tui::panels::cost::render(frame, area, &data, &scroll))
            .unwrap();
        terminal.backend().buffer().clone()
    };

    assert_eq!(render(items.clone()), render(items[..visible].to_vec()));
}

// Feature: terminal-dashboard, Property 4.5: Trends panel renders safely for
// random series and includes the dashboard shell / empty placeholder.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_trends_panel_renders_without_panic(
        points in arb_trend_points(),
        window in prop_oneof![
            Just(TimeWindow::Day24h),
            Just(TimeWindow::Week7d),
            Just(TimeWindow::Month30d),
            Just(TimeWindow::All),
        ],
    ) {
        let text = render_trends_text(points.clone(), window, 100, 26);

        prop_assert!(text.contains("Usage Trends"),
            "Trends panel shell should render for any generated series");
        if points.is_empty() {
            prop_assert!(text.contains("No trend data found."),
                "Empty trend series should render placeholder");
        }
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
        let scroll = ScrollState { offset: 0, total: items.len(), visible: 25 };
        let data: Option<Result<Vec<ModelBreakdown>, String>> = Some(Ok(items.clone()));

        terminal.draw(|frame| {
            llmusage::tui::panels::models::render(frame, area, &data, &scroll);
        }).unwrap();

        let text = buffer_text(&terminal);

        for item in &items {
            prop_assert!(text.contains(&item.model),
                "Missing model name '{}' in output", item.model);
            let tokens_str = format_number(item.total_tokens);
            prop_assert!(text.contains(&tokens_str),
                "Missing total_tokens '{}' in output", tokens_str);
            let events_str = format_number(item.event_count);
            prop_assert!(text.contains(&events_str),
                "Missing event_count '{}' in output", events_str);
            let cost_str = format!("{:.4}", item.cost_with_cache_usd);
            prop_assert!(text.contains(&cost_str),
                "Missing cost_with_cache_usd '{}' in output", cost_str);
        }
    }
}

// Feature: terminal-dashboard, Property 6: Source table renders all required columns
// **Validates: Requirements 6.1**
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_source_table_renders_all_required_columns(
        items in proptest::collection::vec(arb_source_breakdown(), 1..4)
    ) {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let area = Rect::new(0, 0, 120, 30);
        let scroll = ScrollState { offset: 0, total: items.len(), visible: 25 };
        let data: Option<Result<Vec<SourceBreakdown>, String>> = Some(Ok(items.clone()));

        terminal.draw(|frame| {
            llmusage::tui::panels::sources::render(frame, area, &data, &scroll);
        }).unwrap();

        let text = buffer_text(&terminal);

        for item in &items {
            prop_assert!(text.contains(&item.source),
                "Missing source '{}' in output", item.source);
            let tokens_str = format_number(item.total_tokens);
            prop_assert!(text.contains(&tokens_str),
                "Missing total_tokens '{}' in output", tokens_str);
            let events_str = format_number(item.event_count);
            prop_assert!(text.contains(&events_str),
                "Missing event_count '{}' in output", events_str);
            match &item.last_event_at {
                Some(ts) => prop_assert!(text.contains(ts),
                    "Missing last_event_at '{}' in output", ts),
                None => prop_assert!(text.contains("-"),
                    "Missing placeholder '-' for None last_event_at"),
            }
        }
    }
}

// Feature: terminal-dashboard, Property 7: Project table renders all required columns
// **Validates: Requirements 7.1**
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_project_table_renders_all_required_columns(
        items in proptest::collection::vec(arb_project_breakdown(), 1..4)
    ) {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let area = Rect::new(0, 0, 120, 30);
        let scroll = ScrollState { offset: 0, total: items.len(), visible: 25 };
        let data: Option<Result<Vec<ProjectBreakdown>, String>> = Some(Ok(items.clone()));

        terminal.draw(|frame| {
            llmusage::tui::panels::projects::render(frame, area, &data, &scroll);
        }).unwrap();

        let text = buffer_text(&terminal);

        for item in &items {
            prop_assert!(text.contains(&item.project_label),
                "Missing project_label '{}' in output", item.project_label);
            let tokens_str = format_number(item.total_tokens);
            prop_assert!(text.contains(&tokens_str),
                "Missing total_tokens '{}' in output", tokens_str);
            let events_str = format_number(item.event_count);
            prop_assert!(text.contains(&events_str),
                "Missing event_count '{}' in output", events_str);
            let cost_str = format!("{:.4}", item.total_cost_usd);
            prop_assert!(text.contains(&cost_str),
                "Missing total_cost_usd '{}' in output", cost_str);
        }
    }
}

// Feature: terminal-dashboard, Property 8: Cost table renders all required columns
// **Validates: Requirements 8.1**
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_cost_table_renders_all_required_columns(
        items in proptest::collection::vec(arb_cost_line(), 1..4)
    ) {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let area = Rect::new(0, 0, 120, 30);
        let scroll = ScrollState { offset: 0, total: items.len(), visible: 25 };
        let data: Option<Result<Vec<CostLine>, String>> = Some(Ok(items.clone()));

        terminal.draw(|frame| {
            llmusage::tui::panels::cost::render(frame, area, &data, &scroll);
        }).unwrap();

        let text = buffer_text(&terminal);

        for item in &items {
            prop_assert!(text.contains(&item.source),
                "Missing source '{}' in output", item.source);
            prop_assert!(text.contains(&item.model),
                "Missing model '{}' in output", item.model);
            let events_str = format_number(item.event_count);
            prop_assert!(text.contains(&events_str),
                "Missing event_count '{}' in output", events_str);
            let tokens_str = format_number(item.total_tokens);
            prop_assert!(text.contains(&tokens_str),
                "Missing total_tokens '{}' in output", tokens_str);
            let cost_str = format!("${:.2}", item.estimated_cost_usd);
            prop_assert!(text.contains(&cost_str),
                "Missing estimated_cost_usd '{}' in output", cost_str);
        }
    }
}

// Feature: terminal-dashboard, Property 9: Health panel renders all integration and cursor entries
// **Validates: Requirements 9.1, 9.2, 9.3**
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_health_panel_renders_all_entries(payload in arb_health_payload()) {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let area = Rect::new(0, 0, 120, 40);
        let data: Option<Result<HealthPayload, String>> = Some(Ok(payload.clone()));

        terminal.draw(|frame| {
            llmusage::tui::panels::health::render(frame, area, &data);
        }).unwrap();

        let text = buffer_text(&terminal);

        // Every integration's source and status must appear
        for integration in &payload.integrations {
            prop_assert!(text.contains(&integration.source),
                "Missing integration source '{}' in output", integration.source);
            prop_assert!(text.contains(&integration.status),
                "Missing integration status '{}' in output", integration.status);
        }

        // Every cursor's source and cursor_key must appear
        for cursor in &payload.cursors {
            prop_assert!(text.contains(&cursor.source),
                "Missing cursor source '{}' in output", cursor.source);
            prop_assert!(text.contains(&cursor.cursor_key),
                "Missing cursor_key '{}' in output", cursor.cursor_key);
        }

        // At most 10 failure records rendered
        let rendered_failures = payload.recent_failures.iter().take(10).collect::<Vec<_>>();
        for record in &rendered_failures {
            prop_assert!(text.contains(&record.command),
                "Missing failure command '{}' in output", record.command);
        }
        // If more than 10, the 11th should NOT appear (unless its command
        // happens to be a substring of another entry — skip this strict check
        // to avoid false positives with random data)
    }
}
