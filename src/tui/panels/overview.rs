use std::collections::BTreeMap;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::query::DailyModelPoint;
use crate::tui::{
    app::{OverviewPanelPayload, ScrollState, SortState, TableSortKey, stable_sort_refs},
    format::{cost_compact, stat_compact},
    model_vendor::{build_shade_map, vendor_from_model},
    stacked_bar::{StackedBarData, StackedBarSegment, render_stacked_bar_chart},
    theme,
};

const ALL_WINDOW_CHART_DAYS: usize = 60;

/// Render the overview panel as a stacked daily chart and model list.
pub fn render(frame: &mut Frame, area: Rect, data: &Option<Result<OverviewPanelPayload, String>>) {
    let scroll = ScrollState {
        offset: 0,
        selected: 0,
        total: 0,
        visible: 0,
    };
    render_with_plan(frame, area, data, &scroll, SortState::cost_desc());
}

pub(crate) fn render_with_plan(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<OverviewPanelPayload, String>>,
    scroll: &ScrollState,
    sort: SortState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Overview"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Overview"));
            frame.render_widget(widget, area);
        }
        Some(Ok(payload)) => render_payload(frame, area, payload, scroll, sort),
    }
}

fn render_payload(
    frame: &mut Frame,
    area: Rect,
    payload: &OverviewPanelPayload,
    scroll: &ScrollState,
    sort: SortState,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let chart_height = ((area.height as f64) * 0.35).floor().max(5.0) as u16;
    let [chart_area, legend_area, list_area] = Layout::vertical([
        Constraint::Length(chart_height.min(area.height)),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);

    let shade_map = build_shade_map(&payload.models);
    render_stacked_bar_chart(
        frame,
        chart_area,
        &chart_series(&payload.daily_models, &shade_map),
        if area.width < 60 {
            "Tokens"
        } else {
            "Tokens per Day"
        },
    );
    render_legend(frame, legend_area, &payload.models, sort, &shade_map);
    render_model_list(frame, list_area, &payload.models, scroll, sort, &shade_map);
}

fn chart_series(
    points: &[DailyModelPoint],
    shade_map: &std::collections::HashMap<String, usize>,
) -> Vec<StackedBarData> {
    let mut by_date: BTreeMap<&str, Vec<(&str, i64)>> = BTreeMap::new();
    for point in points {
        by_date
            .entry(point.date.as_str())
            .or_default()
            .push((point.model.as_str(), point.total_tokens));
    }
    let start = by_date.len().saturating_sub(ALL_WINDOW_CHART_DAYS);
    by_date
        .into_iter()
        .skip(start)
        .map(|(date, mut models)| {
            models.sort_by(|left, right| left.0.cmp(right.0));
            let total = models.iter().map(|(_, tokens)| *tokens).sum::<i64>();
            let segments = models
                .into_iter()
                .map(|(model, tokens)| {
                    let vendor = vendor_from_model(model);
                    let rank = shade_map.get(model).copied().unwrap_or(0);
                    StackedBarSegment {
                        tokens,
                        color: theme::vendor_fg(vendor, rank),
                    }
                })
                .collect();
            StackedBarData {
                date: date.to_string(),
                total,
                segments,
            }
        })
        .collect()
}

fn render_legend(
    frame: &mut Frame,
    area: Rect,
    models: &[crate::query::ModelBreakdown],
    sort: SortState,
    shade_map: &std::collections::HashMap<String, usize>,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let ordered = sorted_models(models, sort);
    let limit = if area.width < 80 { 3 } else { 5 };
    let name_width = if area.width < 80 { 12 } else { 18 };
    let mut spans = Vec::new();
    for (index, item) in ordered.into_iter().take(limit).enumerate() {
        let vendor = vendor_from_model(&item.model);
        let rank = shade_map.get(&item.model).copied().unwrap_or(0);
        if index > 0 {
            spans.push(Span::styled("  ·", theme::muted_style()));
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("●", theme::vendor_style(vendor, rank)));
        spans.push(Span::styled(
            format!(" {}", truncate_chars(&item.model, name_width)),
            theme::vendor_style(vendor, rank),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_model_list(
    frame: &mut Frame,
    area: Rect,
    models: &[crate::query::ModelBreakdown],
    scroll: &ScrollState,
    sort: SortState,
    shade_map: &std::collections::HashMap<String, usize>,
) {
    let narrow = area.width < 80;
    let very_narrow = area.width < 60;
    let ordered = sorted_models(models, sort);
    let window_cost = ordered
        .iter()
        .map(|item| finite_cost(item.cost_with_cache_usd))
        .sum::<f64>();
    let title = if very_narrow {
        "Top Models".to_string()
    } else if sort.key == Some(TableSortKey::Tokens) {
        "Models by Tokens".to_string()
    } else {
        "Models by Cost".to_string()
    };
    let title_right = if very_narrow {
        cost_compact(window_cost)
    } else {
        format!("Total: {}", cost_compact(window_cost))
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::block_border_style())
        .title(Span::styled(
            format!(" {title} "),
            theme::block_title_style(),
        ))
        .title_top(
            ratatui::text::Line::from(Span::styled(
                format!(" {title_right} "),
                theme::bold_fg_style(theme::positive_fg()),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if ordered.is_empty() {
        frame.render_widget(
            Paragraph::new("No model data found.").style(theme::muted_style()),
            inner,
        );
        return;
    }

    let items_per_page = (inner.height / 2).max(1) as usize;
    let range = scroll.visible_range(ordered.len(), items_per_page);
    let percent_base = window_cost.max(0.01);
    let max_name_width = inner.width.saturating_sub(12).max(8) as usize;
    let mut y = inner.y;
    for absolute in range {
        if y + 1 >= inner.y + inner.height {
            break;
        }
        let item = ordered[absolute];
        let selected = absolute == scroll.selected;
        let row_style = if selected {
            theme::selection_fill_style()
        } else {
            theme::row_style()
        };
        let vendor = vendor_from_model(&item.model);
        let rank = shade_map.get(&item.model).copied().unwrap_or(0);
        let percent = finite_cost(item.cost_with_cache_usd) / percent_base * 100.0;
        let name_style = theme::vendor_style(vendor, rank);

        let line1_area = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(Paragraph::new("").style(row_style), line1_area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("●", name_style),
                Span::styled(
                    format!(" {}", truncate_chars(&item.model, max_name_width)),
                    name_style,
                ),
                Span::styled(format!(" ({percent:.1}%)"), theme::muted_style()),
            ]))
            .style(row_style),
            line1_area,
        );
        y += 1;
        if y >= inner.y + inner.height {
            break;
        }

        let line2_area = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(Paragraph::new("").style(row_style), line2_area);
        let line2 = if narrow {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(stat_compact(item.input_tokens), theme::muted_style()),
                Span::styled("/", theme::muted_style()),
                Span::styled(stat_compact(item.output_tokens), theme::muted_style()),
                Span::styled("/", theme::muted_style()),
                Span::styled(stat_compact(item.cache_read_tokens), theme::muted_style()),
                Span::styled("/", theme::muted_style()),
                Span::styled(
                    stat_compact(item.cache_creation_tokens),
                    theme::muted_style(),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("  In: ", theme::muted_style()),
                Span::styled(
                    stat_compact(item.input_tokens),
                    theme::fg_style(theme::metric_input()),
                ),
                Span::styled(" · Out: ", theme::muted_style()),
                Span::styled(
                    stat_compact(item.output_tokens),
                    theme::fg_style(theme::metric_output()),
                ),
                Span::styled(" · CR: ", theme::muted_style()),
                Span::styled(
                    stat_compact(item.cache_read_tokens),
                    theme::fg_style(theme::metric_cache_read()),
                ),
                Span::styled(" · CW: ", theme::muted_style()),
                Span::styled(
                    stat_compact(item.cache_creation_tokens),
                    theme::fg_style(theme::metric_cache_write()),
                ),
            ])
        };
        frame.render_widget(Paragraph::new(line2).style(row_style), line2_area);
        y += 1;
    }
}

fn sorted_models(
    models: &[crate::query::ModelBreakdown],
    sort: SortState,
) -> Vec<&crate::query::ModelBreakdown> {
    stable_sort_refs(
        models.iter().collect(),
        sort,
        |left, right, key| match key {
            TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
            TableSortKey::Cost => left
                .cost_with_cache_usd
                .total_cmp(&right.cost_with_cache_usd),
            TableSortKey::Date => std::cmp::Ordering::Equal,
        },
    )
}

fn finite_cost(cost: f64) -> f64 {
    if cost.is_finite() { cost.max(0.0) } else { 0.0 }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = value.chars().count();
    if count <= max_chars {
        value.to_string()
    } else if max_chars == 1 {
        "…".to_string()
    } else {
        format!(
            "{}…",
            value
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}

fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::block_border_style())
        .title(Span::styled(
            format!(" {title} "),
            theme::block_title_style(),
        ))
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    use super::*;
    use crate::query::{ModelBreakdown, OverviewPayload, TokenSummary};

    fn model(name: &str, tokens: i64, cost: f64) -> ModelBreakdown {
        ModelBreakdown {
            model: name.to_string(),
            input_tokens: tokens,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: tokens,
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

    fn payload() -> OverviewPanelPayload {
        OverviewPanelPayload {
            totals: OverviewPayload {
                generated_at: "2026-08-19T00:00:00Z".to_string(),
                total: TokenSummary {
                    input_tokens: 10,
                    cache_creation_tokens: 0,
                    cache_read_tokens: 0,
                    output_tokens: 0,
                    reasoning_output_tokens: 0,
                    total_tokens: 10,
                },
                last_24h: TokenSummary::default(),
                source_count: 1,
                bucket_count: 1,
                total_events: 1,
                last_24h_events: 0,
                total_cost_usd: 1.0,
                cache_efficiency: 0.0,
                last_sync_at: None,
                last_export_at: None,
            },
            daily_models: Vec::new(),
            models: vec![model("cheap", 9, 1.0), model("heavy", 1, 9.0)],
        }
    }

    fn render_text(sort: SortState) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let data = Some(Ok(payload()));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 2,
            visible: 4,
        };
        terminal
            .draw(|frame| {
                render_with_plan(frame, Rect::new(0, 0, 100, 24), &data, &scroll, sort);
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn tokens_sort_changes_list_title() {
        let text = render_text(SortState {
            key: Some(TableSortKey::Tokens),
            descending: true,
        });
        assert!(text.contains("Models by Tokens"), "{text}");
        assert!(!text.contains("Models by Cost"), "{text}");
    }

    #[test]
    fn cost_sort_lists_expensive_model_first() {
        let text = render_text(SortState::cost_desc());
        let heavy = text.find("heavy").expect("heavy");
        let cheap = text.find("cheap").expect("cheap");
        assert!(heavy < cheap, "{text}");
    }
}
