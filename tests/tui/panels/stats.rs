use super::super::*;

#[test]
fn stats_panel_renders_year_calendar_and_two_column_stats() {
    let text = render_stats_text(sample_stats_payload(), 120, 30);

    for expected in [
        "Contribution Graph (52 weeks)",
        "Jun",
        "Mon",
        "Favorite model",
        "Events",
        "Current streak",
        "Longest streak",
        "Active days",
        "3/4",
        "gpt-5.5",
        "Context peak",
        "42%",
        "Less",
        "More",
    ] {
        assert!(
            text.contains(expected),
            "stats panel should contain '{expected}', got: {text}"
        );
    }
    assert!(
        text.contains("█"),
        "active days should use two-column cells: {text}"
    );
    for forbidden in [
        "Sessions",
        "Source Mix",
        "Health Signals",
        "#---",
        "08-20 .. 08-19",
        "06-09 .. 06-12",
    ] {
        assert!(
            !text.contains(forbidden),
            "stats panel should not contain '{forbidden}', got: {text}"
        );
    }
}

#[test]
fn stats_panel_uses_wide_labels_at_80() {
    let text = render_stats_text(sample_stats_payload(), 80, 30);

    for expected in [
        "Favorite model",
        "Current streak",
        "Longest streak",
        "Active days",
        "Mon",
    ] {
        assert!(
            text.contains(expected),
            "80-col stats panel should contain '{expected}', got: {text}"
        );
    }
}

#[test]
fn stats_panel_uses_narrow_labels() {
    let text = render_stats_text(sample_stats_payload(), 50, 28);

    for expected in [
        "Model:",
        "Events:",
        "Streak:",
        "Active:",
        "Tokens:",
        "Cost:",
        "Max streak:",
    ] {
        assert!(
            text.contains(expected),
            "narrow stats panel should contain '{expected}', got: {text}"
        );
    }
    assert!(
        !text.contains("Favorite model"),
        "narrow stats panel should hide wide labels: {text}"
    );
}

#[test]
fn stats_panel_favorite_na_and_context_na() {
    let mut payload = sample_stats_payload();
    payload.models.clear();
    payload.context_pressure.priced_events = 0;
    let text = render_stats_text(payload, 120, 30);
    assert!(text.contains("N/A"), "empty models should show N/A: {text}");
    assert!(
        text.contains("n/a"),
        "unpriced context should show n/a: {text}"
    );
}

#[test]
fn stats_panel_day_breakdown_and_empty_day() {
    let payload = sample_stats_payload();
    let detail = PeriodDetailState {
        kind: PeriodDetailKind::Daily {
            date: "2026-06-11".to_string(),
        },
        list_scroll: ScrollState {
            offset: 0,
            selected: 0,
            total: 0,
            visible: 8,
        },
        payload: Some(Ok(PeriodDetailPayload::Daily(vec![PeriodDetailRow {
            model: "gpt-5.5".to_string(),
            source: "codex".to_string(),
            event_count: 3,
            input_tokens: 1_000,
            cache_read_tokens: 100,
            cache_creation_tokens: 50,
            output_tokens: 400,
            total_tokens: 1_550,
            cost_with_cache_usd: 1.25,
        }]))),
    };
    let text = render_stats_text_with_detail(payload, Some(&detail), 120, 30);
    for expected in [
        "Day Breakdown",
        "Jun 11, 2026",
        "codex",
        "gpt-5.5",
        "In ·",
        "Out ·",
        "CR ·",
        "CW ·",
    ] {
        assert!(
            text.contains(expected),
            "day breakdown should contain '{expected}', got: {text}"
        );
    }

    let empty = PeriodDetailState {
        kind: PeriodDetailKind::Daily {
            date: "2026-06-09".to_string(),
        },
        list_scroll: ScrollState {
            offset: 0,
            selected: 0,
            total: 0,
            visible: 8,
        },
        payload: Some(Ok(PeriodDetailPayload::Daily(Vec::new()))),
    };
    let empty_text = render_stats_text_with_detail(sample_stats_payload(), Some(&empty), 120, 30);
    assert!(
        empty_text.contains("No data for this day"),
        "empty day should say so: {empty_text}"
    );
}

#[test]
fn stats_panel_enter_today_and_esc_close() {
    let mut state = AppState::new();
    state.handle_resize(120, 36);
    state.active_panel = Panel::Health;
    state.stats = Some(Ok(sample_stats_payload()));
    state.open_period_detail(PeriodDetailKind::Daily {
        date: "2026-06-12".to_string(),
    });
    state.period_detail.as_mut().unwrap().payload =
        Some(Ok(PeriodDetailPayload::Daily(vec![PeriodDetailRow {
            model: "gpt-5.5".to_string(),
            source: "codex".to_string(),
            event_count: 2,
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 4,
            total_tokens: 14,
            cost_with_cache_usd: 0.2,
        }])));

    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal
        .draw(|frame| llmusage::tui::draw::draw(frame, &state))
        .unwrap();
    let open = buffer_text(&terminal);
    assert!(
        open.contains("Day Breakdown"),
        "enter today should open breakdown: {open}"
    );
    assert!(
        open.contains("codex"),
        "breakdown should list a source: {open}"
    );

    state.close_period_detail();
    terminal
        .draw(|frame| llmusage::tui::draw::draw(frame, &state))
        .unwrap();
    let closed = buffer_text(&terminal);
    assert!(
        !closed.contains("Day Breakdown"),
        "esc should close breakdown: {closed}"
    );
    assert!(
        closed.contains("Contribution Graph (52 weeks)"),
        "closing breakdown must keep Stats open: {closed}"
    );
}

#[test]
fn stats_panel_nocolor_has_no_styles() {
    theme::set_color_mode(theme::TerminalColorMode::NoColor);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    let data = Some(Ok(sample_stats_payload()));
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: 0,
        visible: 8,
    };
    terminal
        .draw(|frame| {
            llmusage::tui::panels::stats::render(
                frame,
                Rect::new(0, 0, 120, 30),
                &data,
                &scroll,
                None,
            );
        })
        .unwrap();
    for cell in terminal.backend().buffer().content() {
        assert_eq!(cell.fg, Color::Reset);
        assert_eq!(cell.bg, Color::Reset);
        assert_eq!(cell.modifier, Modifier::empty());
    }
    theme::set_color_mode(theme::TerminalColorMode::TrueColor);
    theme::set_theme(theme::Theme::default_dark());
}
