use super::*;

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
        "Monthly",
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
    assert!(!text.contains(" 6 Cost"), "Cost tab should be gone: {text}");
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
