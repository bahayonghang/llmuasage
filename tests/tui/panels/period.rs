use super::super::*;

#[test]
fn daily_panel_renders_tokscale_style_token_channels() {
    let text = render_daily_text(
        vec![
            DailyTrendPoint {
                date: "2026-05-28".to_string(),
                input_tokens: 1_000_000_000,
                cache_read_tokens: 2_000_000_000,
                cache_creation_tokens: 500_000_000,
                output_tokens: 14_714_785_227,
                total_tokens: 18_214_785_227,
                event_count: 7,
                cost_with_cache_usd: 1.25,
                turn_count: 0,
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
                turn_count: 0,
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
        "Cache×",
        "Msgs",
        "Cost/1M",
        "18.2B",
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
            turn_count: 0,
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
fn hourly_panel_renders_tokscale_table_and_day_separators() {
    let text = render_hourly_text(
        vec![
            sample_hourly("2026-05-29 13:00", 1_000),
            sample_hourly("2026-05-29 14:00", 18_214_785_227),
        ],
        130,
        14,
    );

    for expected in [
        "Hourly Usage",
        "14:00",
        "05/29",
        "18.2B",
        "Cache×",
        "Source",
    ] {
        assert!(
            text.contains(expected),
            "hourly panel should contain '{expected}', got: {text}"
        );
    }
    assert!(!text.contains("Share"), "hourly should drop Share: {text}");
    assert!(
        !text.contains("Profile"),
        "hourly should drop Profile: {text}"
    );
}

#[test]
fn monthly_visible_window_matches_full_dataset_buffer() {
    let items: Vec<MonthlyTrendPoint> = (0..40)
        .map(|index| sample_monthly(&format!("{:04}", 2040 - index), 10_000, 1.25))
        .collect();
    let visible = 7usize;
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: items.len(),
        visible,
    };
    let area = Rect::new(0, 0, 130, (visible + 4) as u16);

    let render = |rows: Vec<MonthlyTrendPoint>| {
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        let data = Some(Ok(rows));
        terminal
            .draw(|frame| llmusage::tui::panels::monthly::render(frame, area, &data, &scroll))
            .unwrap();
        terminal.backend().buffer().clone()
    };

    assert_eq!(render(items.clone()), render(items[..visible].to_vec()));
}
