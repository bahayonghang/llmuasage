use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::ModelBreakdown;
use crate::tui::{
    format::{cache_multiplier, cost_compact, cost_per_million, stat_compact},
    model_vendor::{build_shade_map, vendor_display_name, vendor_from_model},
    theme,
};

use super::super::app::{ScrollState, SortState, TableSortKey, stable_sort_refs};

/// Render the models panel as a table with scroll support.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<ModelBreakdown>, String>>,
    scroll: &ScrollState,
) {
    render_with_plan(frame, area, data, scroll, SortState::cost_desc());
}

pub(crate) fn render_with_plan(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<ModelBreakdown>, String>>,
    scroll: &ScrollState,
    sort: SortState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Models"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Models"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No model data found.")
                .style(theme::muted_style())
                .block(styled_block("Models"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll, sort),
    }
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    items: &[ModelBreakdown],
    scroll: &ScrollState,
    sort: SortState,
) {
    let tier = width_tier(area.width);
    let header = Row::new(header_cells(tier, sort))
        .style(theme::header_style())
        .bottom_margin(1);

    let ordered = stable_sort_refs(items.iter().collect(), sort, |left, right, key| match key {
        TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
        TableSortKey::Cost => left
            .cost_with_cache_usd
            .total_cmp(&right.cost_with_cache_usd),
        TableSortKey::Date => std::cmp::Ordering::Equal,
    });
    let shade_map = build_shade_map(items);
    let visible_height = super::visible_table_rows(area);
    let range = scroll.visible_range(ordered.len(), visible_height);
    let rows: Vec<Row> = range
        .enumerate()
        .map(|(visible_index, absolute)| {
            let item = ordered[absolute];
            let vendor = vendor_from_model(&item.model);
            let rank = shade_map.get(&item.model).copied().unwrap_or(0);
            let row = Row::new(row_cells(tier, absolute, item, vendor, rank));
            if absolute == scroll.selected {
                row.style(theme::selection_fill_style())
            } else if visible_index % 2 == 1 {
                row.style(theme::row_alt_style())
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(rows, table_widths(tier, area.width))
        .header(header)
        .column_spacing(column_spacing(tier, area.width))
        .block(styled_block("Models"));

    frame.render_widget(table, area);
}

#[derive(Clone, Copy)]
enum WidthTier {
    VeryNarrow,
    Narrow,
    Wide,
}

fn width_tier(width: u16) -> WidthTier {
    if width < 60 {
        WidthTier::VeryNarrow
    } else if width < 80 {
        WidthTier::Narrow
    } else {
        WidthTier::Wide
    }
}

fn header_cells(tier: WidthTier, sort: SortState) -> Vec<Cell<'static>> {
    let labels: Vec<String> = match tier {
        WidthTier::VeryNarrow => vec!["Model".to_string(), sort.header("Cost", TableSortKey::Cost)],
        WidthTier::Narrow => vec![
            "Model".to_string(),
            sort.header("Total", TableSortKey::Tokens),
            sort.header("Cost", TableSortKey::Cost),
        ],
        WidthTier::Wide => vec![
            "#".to_string(),
            "Model".to_string(),
            "Provider".to_string(),
            "Source".to_string(),
            "Input".to_string(),
            "Output".to_string(),
            "Cache R".to_string(),
            "Cache W".to_string(),
            "Cache×".to_string(),
            sort.header("Total", TableSortKey::Tokens),
            "Events".to_string(),
            sort.header("Cost", TableSortKey::Cost),
            "Cost/1M".to_string(),
        ],
    };
    labels.into_iter().map(Cell::from).collect()
}

fn column_spacing(tier: WidthTier, width: u16) -> u16 {
    match tier {
        WidthTier::Wide if width < 120 => 0,
        _ => 1,
    }
}

fn table_widths(tier: WidthTier, width: u16) -> Vec<Constraint> {
    match tier {
        WidthTier::VeryNarrow => vec![Constraint::Percentage(70), Constraint::Percentage(30)],
        WidthTier::Narrow => vec![
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
        // Width 80 leaves 78 inner columns. These lengths plus Model Min(5)
        // fill that row when spacing is 0, so wide headers stay unclipped.
        WidthTier::Wide if width < 120 => vec![
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(7),
        ],
        WidthTier::Wide => vec![
            Constraint::Length(3),
            Constraint::Min(16),
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(9),
        ],
    }
}

fn row_cells(
    tier: WidthTier,
    absolute: usize,
    item: &ModelBreakdown,
    vendor: &str,
    rank: usize,
) -> Vec<Cell<'static>> {
    let model = Cell::from(item.model.clone()).style(theme::vendor_style(vendor, rank));
    let cost = Cell::from(cost_compact(item.cost_with_cache_usd))
        .style(theme::fg_style(theme::positive_fg()));
    match tier {
        WidthTier::VeryNarrow => vec![model, cost],
        WidthTier::Narrow => vec![model, Cell::from(stat_compact(item.total_tokens)), cost],
        WidthTier::Wide => vec![
            Cell::from((absolute + 1).to_string()).style(theme::muted_style()),
            model,
            Cell::from(vendor_display_name(vendor)),
            Cell::from(item.sources.join(", ")).style(theme::muted_style()),
            Cell::from(stat_compact(item.input_tokens))
                .style(theme::fg_style(theme::metric_input())),
            Cell::from(stat_compact(item.output_tokens))
                .style(theme::fg_style(theme::metric_output())),
            Cell::from(stat_compact(item.cache_read_tokens))
                .style(theme::fg_style(theme::metric_cache_read())),
            Cell::from(stat_compact(item.cache_creation_tokens))
                .style(theme::fg_style(theme::metric_cache_write())),
            Cell::from(cache_multiplier(
                item.cache_read_tokens,
                item.input_tokens,
                item.cache_creation_tokens,
            ))
            .style(theme::fg_style(theme::metric_cache_hit())),
            Cell::from(stat_compact(item.total_tokens)),
            Cell::from(stat_compact(item.event_count)),
            cost,
            Cell::from(cost_per_million(
                item.cost_with_cache_usd,
                item.total_tokens,
            ))
            .style(theme::fg_style(theme::metric_cost_per_million())),
        ],
    }
}

fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::block_border_style())
        .title(Span::styled(
            format!(" {} ", title),
            theme::block_title_style(),
        ))
}
