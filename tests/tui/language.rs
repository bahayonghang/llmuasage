use super::*;
use llmusage::common::i18n::{Language, with_language};
use unicode_width::UnicodeWidthStr;

fn render_ref(state: &mut AppState, width: u16, height: u16) -> String {
    state.handle_resize(width, height);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| llmusage::tui::draw::draw(frame, state))
        .unwrap();
    buffer_text(&terminal)
}

fn populated_state(panel: Panel) -> AppState {
    let mut state = AppState::new();
    state.active_panel = panel;
    state.overview = Some(Ok(sample_overview_payload()));
    state.quota_report = Some(sample_quota_report());
    state.models = Some(Ok(vec![sample_model("MODEL_SENTINEL", 12345, 1.25)]));
    state.daily = Some(Ok(Vec::new()));
    state.hourly = Some(Ok(vec![sample_hourly("2026-08-19 12:00", 12345)]));
    state.monthly = Some(Ok(vec![sample_monthly("2026-08", 12345, 1.25)]));
    state.stats = Some(Ok(sample_stats_payload()));
    state.behavior = Some(Ok(sample_behavior_payload()));
    state.blocks = Some(Ok(Vec::new()));
    state.sync_center = Some(Ok(sample_sync_payload()));
    state
}

#[test]
fn all_nine_panels_render_paired_loading_data_and_empty_states() {
    for (language, loading, titles) in [
        (
            Language::English,
            "Loading",
            [
                "Overview", "Usage", "Models", "Daily", "Hourly", "Monthly", "Stats", "Behavior",
                "Blocks",
            ],
        ),
        (
            Language::Chinese,
            "加载",
            [
                "概览", "额度", "模型", "每日", "每时", "每月", "统计", "行为", "时段",
            ],
        ),
    ] {
        with_language(language, || {
            for (index, panel) in Panel::all().iter().copied().enumerate() {
                let mut empty = AppState::new();
                empty.active_panel = panel;
                let loading_text = render_shell_text(empty, 180, 50);
                assert!(
                    loading_text.contains(titles[index]),
                    "{panel:?}: {loading_text}"
                );
                // Usage has its own no-credentials/loading branch, covered by quota tests.
                if panel != Panel::Trends {
                    assert!(loading_text.contains(loading), "{panel:?}: {loading_text}");
                }
                let text = render_shell_text(populated_state(panel), 180, 50);
                assert!(text.contains(titles[index]), "{panel:?}: {text}");
                if matches!(panel, Panel::Models | Panel::Projects | Panel::Monthly) {
                    assert!(text.contains("12.3K"), "numeric formatter changed: {text}");
                    assert!(text.contains("$1.25"), "cost formatter changed: {text}");
                }
                if language == Language::Chinese {
                    assert!(!text.contains("Loading..."), "{panel:?}: {text}");
                    assert!(!text.contains("No daily usage data"), "{text}");
                    assert!(!text.contains("No block data"), "{text}");
                    assert!(!text.contains("Recommendation:"), "{text}");
                }
            }
        });
    }
}

#[test]
fn every_analytics_error_keeps_original_detail_with_paired_context() {
    for (language, label) in [
        (Language::English, "Data load failed"),
        (Language::Chinese, "数据加载失败"),
    ] {
        with_language(language, || {
            for panel in Panel::all()
                .iter()
                .copied()
                .filter(|panel| *panel != Panel::Trends)
            {
                let mut state = AppState::new();
                state.active_panel = panel;
                state.overview = Some(Err("OS_SENTINEL".into()));
                state.models = Some(Err("OS_SENTINEL".into()));
                state.daily = Some(Err("OS_SENTINEL".into()));
                state.hourly = Some(Err("OS_SENTINEL".into()));
                state.monthly = Some(Err("OS_SENTINEL".into()));
                state.stats = Some(Err("OS_SENTINEL".into()));
                state.behavior = Some(Err("OS_SENTINEL".into()));
                state.blocks = Some(Err("OS_SENTINEL".into()));
                let text = render_shell_text(state, 150, 40);
                assert!(
                    text.contains(label) && text.contains("OS_SENTINEL"),
                    "{panel:?}: {text}"
                );
            }
        });
    }
}

#[test]
fn chinese_nav_paint_and_hit_ranges_share_display_width_including_clipping() {
    for language in [Language::English, Language::Chinese] {
        with_language(language, || {
            for width in [40, 59, 60, 80, 120, 180] {
                let area = Rect::new(3, 2, width, 3);
                let mut terminal = Terminal::new(TestBackend::new(width + 6, 7)).unwrap();
                terminal
                    .draw(|frame| llmusage::tui::nav_bar::render(frame, area, Panel::Behavior))
                    .unwrap();
                let mut x = area.x + 1;
                let right = area.right() - 1;
                for (index, panel) in Panel::all().iter().copied().enumerate() {
                    if index > 0 {
                        for column in x..x.saturating_add(3).min(right) {
                            assert_eq!(
                                llmusage::tui::nav_bar::panel_at_position(area, column, 3),
                                None
                            );
                        }
                        x += 3;
                    }
                    if x >= right {
                        break;
                    }
                    let label = format!(
                        " {} {} ",
                        index + 1,
                        if width < 60 {
                            panel.short_label()
                        } else {
                            panel.label()
                        }
                    );
                    let end = (x + label.width() as u16).min(right);
                    for column in x..end {
                        assert_eq!(
                            llmusage::tui::nav_bar::panel_at_position(area, column, 3),
                            Some(panel),
                            "{language:?} width={width} column={column}"
                        );
                    }
                    let visible_digit = terminal.backend().buffer()[(x + 1, 3)].symbol();
                    if x + 1 < right {
                        assert_eq!(visible_digit, (index + 1).to_string());
                    }
                    x = end;
                }
                for (column, row) in [(area.x, 3), (right, 3), (4, 2), (4, 4)] {
                    assert_eq!(
                        llmusage::tui::nav_bar::panel_at_position(area, column, row),
                        None
                    );
                }
            }
        });
    }
}

#[test]
fn dialog_footer_and_sync_overlay_branches_are_paired() {
    for (language, help, sources, sync, back, detail) in [
        (
            Language::English,
            "Help / Settings",
            "Source picker",
            "Source Sync",
            "[esc:back]",
            "[enter:detail]",
        ),
        (
            Language::Chinese,
            "帮助 / 设置",
            "来源选择",
            "来源同步",
            "[esc:返回]",
            "[enter:详情]",
        ),
    ] {
        with_language(language, || {
            for (dialog, expected) in [
                (ActiveDialog::Help, help),
                (ActiveDialog::SourcePicker, sources),
                (ActiveDialog::SyncStatus, sync),
            ] {
                let mut state = populated_state(Panel::Trends);
                state.active_dialog = Some(dialog);
                let text = render_shell_text(state, 160, 50);
                assert!(text.contains(expected), "{text}");
            }
            let mut state = populated_state(Panel::Sources);
            assert!(render_ref(&mut state, 220, 40).contains(detail));
            state.open_period_detail(PeriodDetailKind::Daily {
                date: "2026-08-19".into(),
            });
            let text = render_ref(&mut state, 220, 40);
            assert!(text.contains(back) && text.contains("2026-08-19"), "{text}");
            state.period_detail.as_mut().unwrap().payload =
                Some(Ok(PeriodDetailPayload::Daily(Vec::new())));
            let text = render_shell_text(state, 220, 40);
            assert!(
                text.contains(match language {
                    Language::English => "No model details",
                    Language::Chinese => "没有模型详情",
                }),
                "{text}"
            );
            for width in [50, 70, 120] {
                let mut state = populated_state(Panel::Models);
                state.auto_refresh = true;
                assert!(render_shell_text(state, width, 25).contains("o/O"));
            }
        });
    }
}

#[test]
fn language_changes_labels_while_hotkeys_sort_and_selection_stay_identical() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use llmusage::tui::input::handle_key_event;
    for code in [
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Char('o'),
        KeyCode::Char('O'),
        KeyCode::Enter,
        KeyCode::Esc,
    ] {
        let key = KeyEvent::new(code, KeyModifiers::NONE);
        let en = with_language(Language::English, || handle_key_event(key, Panel::Models));
        let zh = with_language(Language::Chinese, || handle_key_event(key, Panel::Models));
        assert_eq!(en, zh);
    }
    for language in [Language::English, Language::Chinese] {
        with_language(language, || {
            let mut state = populated_state(Panel::Models);
            let sort = state.cycle_sort();
            assert!(sort.is_some());
            let reversed = state.reverse_sort().unwrap();
            assert_eq!(sort.unwrap().0, reversed.0);
            assert_ne!(sort.unwrap().1, reversed.1);
            state.scroll[Panel::Models as usize].set_total(3);
            state.scroll[Panel::Models as usize].scroll_up();
            assert_eq!(state.scroll[Panel::Models as usize].selected, 2);
            state.scroll[Panel::Models as usize].scroll_down();
            assert_eq!(state.scroll[Panel::Models as usize].selected, 0);
            let text = render_shell_text(state, 180, 40);
            assert!(text.contains('▲') || text.contains('▼'), "{text}");
        });
    }
}

#[test]
fn chinese_rendering_preserves_no_color_across_panels_and_dialogs() {
    use llmusage::tui::theme::TerminalColorMode;
    theme::set_color_mode(TerminalColorMode::NoColor);
    with_language(Language::Chinese, || {
        for panel in Panel::all().iter().copied() {
            for dialog in [
                None,
                Some(ActiveDialog::Help),
                Some(ActiveDialog::SourcePicker),
                Some(ActiveDialog::SyncStatus),
            ] {
                let mut state = populated_state(panel);
                state.active_dialog = dialog;
                let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
                terminal
                    .draw(|frame| llmusage::tui::draw::draw(frame, &state))
                    .unwrap();
                for cell in terminal.backend().buffer().content() {
                    assert_eq!(cell.fg, Color::Reset);
                    assert_eq!(cell.bg, Color::Reset);
                    assert!(cell.modifier.is_empty());
                }
            }
        }
    });
    theme::set_color_mode(TerminalColorMode::TrueColor);
}
