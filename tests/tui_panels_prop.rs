//! Property-based tests for TUI panel rendering.
//! Feature: terminal-dashboard, Properties 2, 5, 6, 7, 8, 9
//!
//! Uses `proptest` to generate random data structs and `ratatui::Terminal`
//! with `TestBackend` to render panels into a buffer, then asserts that
//! expected strings appear in the rendered output.

use proptest::prelude::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use llmusage::query::{
    CostLine, CursorHealth, HealthPayload, ModelBreakdown, OverviewPayload, ProjectBreakdown,
    SourceBreakdown, TokenSummary, TrendPoint,
};
use llmusage::store::{IntegrationState, RunRecord};
use llmusage::tui::app::{ScrollState, TimeWindow};

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
    (0i64..100_000, 0i64..100_000, 0i64..100_000, 0i64..100_000).prop_map(
        |(input, cache, output, reasoning)| TokenSummary {
            input_tokens: input,
            cache_read_tokens: cache,
            output_tokens: output,
            reasoning_output_tokens: reasoning,
            total_tokens: input + cache + output + reasoning,
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

// ─── Property Tests ───────────────────────────────────────────────────────────

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
        text.contains("暂无趋势数据"),
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
        text.contains("趋势"),
        "small terminal should render a shell"
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

        // last_sync_at or "从未同步"
        match &payload.last_sync_at {
            Some(ts) => prop_assert!(text.contains(ts),
                "Missing last_sync_at '{}' in output", ts),
            None => prop_assert!(text.contains("从未同步"),
                "Missing '从未同步' placeholder in output"),
        }
    }
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

        prop_assert!(text.contains("趋势"),
            "Trends panel shell should render for any generated series");
        if points.is_empty() {
            prop_assert!(text.contains("暂无趋势数据"),
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
