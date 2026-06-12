use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
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

    if inner.height < 16 {
        let [kpi_area, _gap, meta_area] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(inner);
        render_kpi_row(frame, kpi_area, payload);
        render_compact_metadata(frame, meta_area, payload);
        return;
    }

    let detail_height = if inner.width < 90 { 15 } else { 8 };
    let [kpi_area, _gap, detail_area, _gap2, pulse_area] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(detail_height),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);

    render_kpi_row(frame, kpi_area, payload);
    render_detail_sections(frame, detail_area, payload);
    render_24h_pulse(frame, pulse_area, payload);
}

fn render_kpi_row(frame: &mut Frame, area: Rect, payload: &OverviewPayload) {
    let kpi_cols: [Rect; 4] = Layout::horizontal([
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
    ])
    .areas(area);

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
}

fn render_detail_sections(frame: &mut Frame, area: Rect, payload: &OverviewPayload) {
    if area.width < 90 {
        let [tokens, activity, freshness] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .areas(area);
        render_summary_block(frame, tokens, "Token Mix", token_mix_lines(&payload.total));
        render_summary_block(frame, activity, "Recent Activity", activity_lines(payload));
        render_summary_block(frame, freshness, "Freshness", freshness_lines(payload));
        return;
    }

    let [tokens, activity, freshness] = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .areas(area);
    render_summary_block(frame, tokens, "Token Mix", token_mix_lines(&payload.total));
    render_summary_block(frame, activity, "Recent Activity", activity_lines(payload));
    render_summary_block(frame, freshness, "Freshness", freshness_lines(payload));
}

fn render_24h_pulse(frame: &mut Frame, area: Rect, payload: &OverviewPayload) {
    if area.height == 0 {
        return;
    }

    let share = percentage(payload.last_24h.total_tokens, payload.total.total_tokens);
    let avg = average_tokens(payload.last_24h.total_tokens, payload.last_24h_events);
    let mut lines = vec![
        metric_line(
            "Tokens",
            format_tokens(payload.last_24h.total_tokens),
            Color::Green,
        ),
        metric_line(
            "Events",
            format_tokens(payload.last_24h_events),
            Color::Cyan,
        ),
        metric_line("Avg/event", avg, Color::Yellow),
        metric_line("All-time share", share, Color::Magenta),
    ];
    lines.extend(token_mix_lines(&payload.last_24h));

    render_summary_block(frame, area, "24h Pulse", lines);
}

fn render_compact_metadata(frame: &mut Frame, area: Rect, payload: &OverviewPayload) {
    let meta_lines = vec![
        metric_line("Sources", payload.source_count.to_string(), theme::ACCENT),
        metric_line("Buckets", payload.bucket_count.to_string(), theme::ACCENT),
        metric_line("Last sync", last_sync_text(payload), theme::ACCENT),
    ];
    frame.render_widget(Paragraph::new(meta_lines), area);
}

fn render_summary_block(frame: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'static>>) {
    let widget = Paragraph::new(lines).block(theme::panel_block(title));
    frame.render_widget(widget, area);
}

fn token_mix_lines(summary: &crate::query::TokenSummary) -> Vec<Line<'static>> {
    vec![
        metric_line("Input", format_tokens(summary.input_tokens), Color::Cyan),
        metric_line("Output", format_tokens(summary.output_tokens), Color::Green),
        metric_line(
            "Cache read",
            format_tokens(summary.cache_read_tokens),
            Color::Blue,
        ),
        metric_line(
            "Cache write",
            format_tokens(summary.cache_creation_tokens),
            Color::Magenta,
        ),
        metric_line(
            "Reasoning",
            format_tokens(summary.reasoning_output_tokens),
            Color::Yellow,
        ),
    ]
}

fn activity_lines(payload: &OverviewPayload) -> Vec<Line<'static>> {
    vec![
        metric_line("Events", format_tokens(payload.total_events), Color::Green),
        metric_line(
            "24h events",
            format_tokens(payload.last_24h_events),
            Color::Cyan,
        ),
        metric_line(
            "Avg/event",
            average_tokens(payload.total.total_tokens, payload.total_events),
            Color::Yellow,
        ),
        metric_line("Sources", payload.source_count.to_string(), Color::Magenta),
        metric_line("Buckets", payload.bucket_count.to_string(), Color::Blue),
    ]
}

fn freshness_lines(payload: &OverviewPayload) -> Vec<Line<'static>> {
    vec![
        metric_line("Last sync", last_sync_text(payload), Color::Green),
        metric_line(
            "Last export",
            payload
                .last_export_at
                .clone()
                .unwrap_or_else(|| "never".to_string()),
            Color::Cyan,
        ),
        metric_line("Generated", payload.generated_at.clone(), Color::Blue),
        metric_line(
            "Cache hit",
            format!("{:.1}%", payload.cache_efficiency * 100.0),
            Color::Magenta,
        ),
    ]
}

fn metric_line(label: &'static str, value: String, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<12}"), theme::muted_style()),
        Span::styled(
            value,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn last_sync_text(payload: &OverviewPayload) -> String {
    payload
        .last_sync_at
        .clone()
        .unwrap_or_else(|| "从未同步".to_string())
}

fn average_tokens(tokens: i64, events: i64) -> String {
    if events <= 0 {
        "-".to_string()
    } else {
        format_tokens(tokens / events)
    }
}

fn percentage(part: i64, total: i64) -> String {
    if total <= 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", (part as f64 / total as f64) * 100.0)
    }
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
