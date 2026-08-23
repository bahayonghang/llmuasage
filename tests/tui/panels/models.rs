use super::super::*;

#[test]
fn models_visible_window_matches_full_dataset_buffer() {
    let items: Vec<ModelBreakdown> = (0..40)
        .map(|index| ModelBreakdown {
            model: format!("modelx{index:02}"),
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
            sources: Vec::new(),
        })
        .collect();
    let visible = 7usize;
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
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
fn models_wide_headers_default_cost_sort_and_no_long_tail_fold() {
    let items: Vec<ModelBreakdown> = (0..20)
        .map(|index| {
            sample_model(
                &format!("tail-{index:02}"),
                1_000 + i64::from(index),
                f64::from(index),
            )
        })
        .collect();
    let text = render_models_text(items.clone(), 160, 30);
    for header in [
        "#", "Model", "Provider", "Source", "Input", "Output", "Cache R", "Cache W", "Cache×",
        "Total", "Events", "Cost ▼", "Cost/1M",
    ] {
        assert!(text.contains(header), "missing header {header} in {text}");
    }
    assert!(!text.contains(" more ·"));
    let high = text.find("tail-19").expect("highest-cost row");
    let low = text.find("tail-00").expect("lowest-cost row");
    assert!(
        high < low,
        "rows must follow cost_with_cache_usd descending"
    );

    let min_wide = render_models_text(items, 80, 16);
    for header in [
        "Provider", "Source", "Input", "Output", "Cache R", "Cache W", "Cache×", "Total", "Events",
        "Cost", "Cost/1M",
    ] {
        assert!(
            min_wide.contains(header),
            "width 80 must keep header {header} in {min_wide}"
        );
    }
}

#[test]
fn models_wide_paints_inferred_provider_and_joined_sources() {
    let mut item = sample_model("gpt-4o", 12_500, 12.59);
    item.sources = vec!["claude".to_string(), "codex".to_string()];
    let text = render_models_text(vec![item], 160, 12);
    assert!(
        text.contains("OpenAI"),
        "provider display missing in {text}"
    );
    assert!(
        text.contains("claude, codex"),
        "joined sources missing in {text}"
    );
}

#[test]
fn models_narrow_and_very_narrow_drop_identity_columns() {
    let items = vec![sample_model("gpt-4o", 12_500, 12.59)];
    let very_narrow = render_models_text(items.clone(), 59, 12);
    assert!(very_narrow.contains("Model"));
    assert!(very_narrow.contains("Cost"));
    assert!(!very_narrow.contains("Provider"));
    assert!(!very_narrow.contains("Cache×"));
    assert!(!very_narrow.contains("Events"));
    assert!(!very_narrow.contains("Total"));

    let narrow = render_models_text(items, 79, 12);
    assert!(narrow.contains("Model"));
    assert!(narrow.contains("Total"));
    assert!(narrow.contains("Cost"));
    assert!(!narrow.contains("Provider"));
    assert!(!narrow.contains("Cache×"));
}

#[test]
fn models_nocolor_has_no_styles() {
    theme::set_color_mode(theme::TerminalColorMode::NoColor);
    theme::set_theme(theme::Theme::graphite());
    let items = vec![
        sample_model("claude-opus-4", 1000, 2.0),
        sample_model("gpt-4o", 2000, 3.0),
    ];
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    let area = Rect::new(0, 0, 120, 20);
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: items.len(),
        visible: 16,
    };
    let data = Some(Ok(items));
    terminal
        .draw(|frame| {
            llmusage::tui::panels::models::render(frame, area, &data, &scroll);
        })
        .unwrap();
    for cell in terminal.backend().buffer().content() {
        assert_eq!(cell.fg, ratatui::style::Color::Reset);
        assert_eq!(cell.bg, ratatui::style::Color::Reset);
        assert_eq!(cell.modifier, ratatui::style::Modifier::empty());
    }
    theme::set_color_mode(theme::TerminalColorMode::TrueColor);
    theme::set_theme(theme::Theme::default_dark());
}
