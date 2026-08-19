use ratatui::{
    Frame,
    layout::Rect,
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::MonthlyTrendPoint;
use crate::tui::app::{PeriodDetailState, ScrollState, SortState, TableSortKey, stable_sort_refs};
use crate::tui::theme;

use super::daily;
use super::period::{
    PeriodValues, WidthTier, period_has_turns, period_header_row, period_metric_cells,
    period_widths, width_tier,
};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<MonthlyTrendPoint>, String>>,
    scroll: &ScrollState,
) {
    render_sorted(frame, area, data, scroll, SortState::date_desc(), None);
}

pub(crate) fn render_sorted(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<MonthlyTrendPoint>, String>>,
    scroll: &ScrollState,
    sort: SortState,
    detail: Option<&PeriodDetailState>,
) {
    if let Some(detail) = detail {
        daily::render_sorted(frame, area, &None, scroll, sort, Some(detail));
        return;
    }
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Monthly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Monthly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No monthly usage data found. Press r to refresh.")
                .style(theme::muted_style())
                .block(styled_block("Monthly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll, sort),
    }
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    items: &[MonthlyTrendPoint],
    scroll: &ScrollState,
    sort: SortState,
) {
    let block = styled_block("Monthly Usage");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let tier = width_tier(inner.width);
    let has_turns = period_has_turns(items.iter().map(|month| month.turn_count));
    let header = period_header_row("Month", &[], has_turns, tier, sort);
    let ordered = stable_sort_refs(items.iter().collect(), sort, |left, right, key| match key {
        TableSortKey::Date => left.month.cmp(&right.month),
        TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
        TableSortKey::Cost => left
            .cost_with_cache_usd
            .total_cmp(&right.cost_with_cache_usd),
    });
    let visible_height = super::visible_table_rows(area);
    let range = scroll.visible_range(ordered.len(), visible_height);
    let rows = range.enumerate().map(|(visible_index, absolute)| {
        let month = ordered[absolute];
        let mut cells = vec![Cell::from(month.month.clone()).style(theme::bold_style())];
        cells.extend(period_metric_cells(
            &PeriodValues {
                turn_count: month.turn_count,
                event_count: month.event_count,
                input_tokens: month.input_tokens,
                output_tokens: month.output_tokens,
                cache_read_tokens: month.cache_read_tokens,
                cache_creation_tokens: month.cache_creation_tokens,
                total_tokens: month.total_tokens,
                cost_with_cache_usd: month.cost_with_cache_usd,
            },
            Vec::new(),
            has_turns,
            tier,
        ));
        let row = Row::new(cells);
        if absolute == scroll.selected {
            row.style(theme::selection_style())
        } else if visible_index % 2 == 1 {
            row.style(theme::row_alt_style())
        } else {
            row
        }
    });
    let time_width = if matches!(tier, WidthTier::Wide) && inner.width < 112 {
        7
    } else {
        12
    };
    let table = Table::new(rows, period_widths(0, has_turns, tier, time_width)).header(header);
    frame.render_widget(table, inner);
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
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn wide_table_uses_month_and_tokscale_columns() {
        let data = Some(Ok(vec![MonthlyTrendPoint {
            month: "2026-07".to_string(),
            input_tokens: 10,
            cache_read_tokens: 4,
            cache_creation_tokens: 1,
            output_tokens: 2,
            total_tokens: 17,
            event_count: 3,
            turn_count: 2,
            cost_with_cache_usd: 1.5,
        }]));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 1,
            visible: 6,
        };
        let mut terminal = Terminal::new(TestBackend::new(130, 10)).unwrap();
        terminal
            .draw(|frame| {
                render_sorted(
                    frame,
                    frame.area(),
                    &data,
                    &scroll,
                    SortState::date_desc(),
                    None,
                )
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Month"), "{text}");
        assert!(text.contains("2026-07"), "{text}");
        assert!(text.contains("Msgs"), "{text}");
        assert!(text.contains("Cache×"), "{text}");
        assert!(text.contains("Cost/1M"), "{text}");
        assert!(text.contains("Turn"), "{text}");
    }
}
