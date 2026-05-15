use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::query::OverviewPayload;
use crate::tui::theme;

/// Render the overview panel with KPI cards and metadata.
pub fn render(frame: &mut Frame, area: Rect, data: &Option<Result<OverviewPayload, String>>) {
    match data {
        None => {
            let widget = Paragraph::new("加载中...")
                .style(theme::muted_style())
                .block(styled_block("概览"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("数据加载失败: {e}"))
                .style(theme::error_style())
                .block(styled_block("概览"));
            frame.render_widget(widget, area);
        }
        Some(Ok(payload)) => render_payload(frame, area, payload),
    }
}

fn render_payload(frame: &mut Frame, area: Rect, payload: &OverviewPayload) {
    let block = styled_block("概览");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split into KPI row and metadata row
    let [kpi_area, _gap, meta_area] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);

    // KPI cards: 4 columns
    let kpi_cols: [Rect; 4] = Layout::horizontal([
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
    ])
    .areas(kpi_area);

    render_kpi_card(
        frame,
        kpi_cols[0],
        "累计 Tokens",
        &format_tokens(payload.total.total_tokens),
        theme::KPI_COLORS[0],
    );
    render_kpi_card(
        frame,
        kpi_cols[1],
        "24h Tokens",
        &format_tokens(payload.last_24h.total_tokens),
        theme::KPI_COLORS[1],
    );
    render_kpi_card(
        frame,
        kpi_cols[2],
        "累计成本",
        &format!("${:.2}", payload.total_cost_usd),
        theme::KPI_COLORS[2],
    );
    render_kpi_card(
        frame,
        kpi_cols[3],
        "缓存命中率",
        &format!("{:.1}%", payload.cache_efficiency * 100.0),
        theme::KPI_COLORS[3],
    );

    // Metadata lines
    let last_sync = match &payload.last_sync_at {
        Some(ts) => ts.clone(),
        None => "从未同步".to_string(),
    };

    let meta_lines = vec![
        Line::from(vec![
            Span::styled(
                "来源数: ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(payload.source_count.to_string()),
            Span::raw("    "),
            Span::styled(
                "桶数: ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(payload.bucket_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled(
                "最近同步: ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(last_sync),
        ]),
    ];

    let meta_widget = Paragraph::new(meta_lines);
    frame.render_widget(meta_widget, meta_area);
}

fn render_kpi_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    color: ratatui::style::Color,
) {
    let card = Paragraph::new(Line::from(vec![Span::styled(
        value,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .title(Span::styled(
                format!(" {} ", title),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(card, area);
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

/// Format token counts with thousands separators for readability.
fn format_tokens(n: i64) -> String {
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
