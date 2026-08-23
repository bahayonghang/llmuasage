use super::super::*;

#[test]
fn overview_panel_renders_chart_and_cost_list() {
    let payload = sample_overview_payload();
    let text = render_overview_text(payload, 120, 30);

    for expected in [
        "Tokens per Day",
        "Models by Cost",
        "Total:",
        "gpt-5.5",
        "claude-opus-5",
        "In:",
        "Out:",
        "CR:",
        "CW:",
        "640.4M",
        "6.3B",
    ] {
        assert!(
            text.contains(expected),
            "overview panel should contain '{expected}', got: {text}"
        );
    }
    for unexpected in ["Token Mix", "24h Pulse", "Freshness", "Total Tokens"] {
        assert!(
            !text.contains(unexpected),
            "overview panel should not contain '{unexpected}', got: {text}"
        );
    }
}

#[test]
fn overview_panel_compacts_screenshot_scale_statistics_in_wide_and_narrow_layouts() {
    let payload = sample_overview_payload();

    let wide = render_overview_text(payload.clone(), 120, 30);
    for expected in [
        "640.4M", "6.3B", "874M", "107.2M", "Total:", "$9.5K", "86.7%",
    ] {
        assert!(
            wide.contains(expected),
            "wide overview should contain '{expected}', got: {wide}"
        );
    }
    assert!(
        !wide.contains("640,400,000"),
        "wide overview should not contain exact input, got: {wide}"
    );

    let narrow = render_overview_text(payload, 70, 30);
    assert!(
        narrow.contains("640.4M"),
        "narrow overview should compact input, got: {narrow}"
    );
    assert!(
        !narrow.contains("In:"),
        "narrow overview should use slash token mix, got: {narrow}"
    );
}

#[test]
fn overview_panel_empty_models_and_no_long_tail() {
    let mut payload = sample_overview_payload();
    payload.models.clear();
    payload.daily_models.clear();
    let empty = render_overview_text(payload, 120, 30);
    assert!(
        empty.contains("No model data found."),
        "empty overview should explain missing models, got: {empty}"
    );
    assert!(
        !empty.contains("+N more") && !empty.contains("more ·"),
        "overview must not fold a long tail, got: {empty}"
    );
}

#[test]
fn overview_panel_nocolor_has_no_styles() {
    theme::set_color_mode(theme::TerminalColorMode::NoColor);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    let data = Some(Ok(sample_overview_payload()));
    terminal
        .draw(|frame| {
            llmusage::tui::panels::overview::render(frame, Rect::new(0, 0, 120, 30), &data);
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
