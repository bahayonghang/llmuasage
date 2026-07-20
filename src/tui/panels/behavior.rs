use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::query::{BehaviorSupport, ModelComparePayload, OptimizePayload, ZombieReport};
use crate::tui::{
    app::BehaviorPanelPayload,
    format::{
        cost as format_cost, grouped as format_number, metric_value as format_metric_value,
        percent_ratio as format_percent,
    },
    theme,
};

/// Render the terminal behavior analytics summary.
pub fn render(frame: &mut Frame, area: Rect, data: &Option<Result<BehaviorPanelPayload, String>>) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(theme::panel_block("Behavior"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("Behavior"));
            frame.render_widget(widget, area);
        }
        Some(Ok(payload)) => render_payload(frame, area, payload),
    }
}

fn render_payload(frame: &mut Frame, area: Rect, payload: &BehaviorPanelPayload) {
    let [activity_area, tools_area, optimize_area, compare_area] = Layout::vertical([
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ])
    .areas(area);

    render_activity(frame, activity_area, payload);
    render_tools(frame, tools_area, payload);
    render_optimize(frame, optimize_area, &payload.optimize, &payload.zombie);
    render_compare(frame, compare_area, &payload.compare);
}

fn render_activity(frame: &mut Frame, area: Rect, payload: &BehaviorPanelPayload) {
    let mut lines = vec![support_line(&payload.activity.support)];
    if payload.activity.breakdown.is_empty() {
        lines.push(empty_reason_line(
            &payload.activity.support,
            "No activity behavior facts.",
        ));
    } else {
        lines.extend(payload.activity.breakdown.iter().take(4).map(|row| {
            Line::from(vec![
                Span::styled(
                    format!("{} ", row.category),
                    theme::bold_fg_style(theme::accent()),
                ),
                Span::raw(format!(
                    "turns={} edits={} tokens={} cost={} one-shot={} retry={}",
                    format_number(row.turns),
                    format_number(row.edit_turns),
                    format_number(row.total_tokens),
                    format_cost(row.estimated_cost_usd),
                    format_percent(row.one_shot_rate),
                    format_percent(row.retry_rate),
                )),
            ])
        }));
    }
    render_section(frame, area, "Behavior / Activity Categories", lines);
}

fn render_tools(frame: &mut Frame, area: Rect, payload: &BehaviorPanelPayload) {
    let mut lines = vec![support_line(&payload.tools.support)];
    if payload.tools.breakdown.is_empty() {
        lines.push(empty_reason_line(
            &payload.tools.support,
            "No tool behavior facts.",
        ));
    } else {
        lines.extend(payload.tools.breakdown.iter().take(4).map(|row| {
            let server = row
                .mcp_server
                .as_deref()
                .map(|server| format!(" @{server}"))
                .unwrap_or_default();
            Line::from(vec![
                Span::styled(
                    format!("{}/{}{} ", row.tool_kind, row.tool_name, server),
                    theme::bold_fg_style(theme::accent()),
                ),
                Span::raw(format!(
                    "calls={} turns={} sessions={} share={} cost={}",
                    format_number(row.calls),
                    format_number(row.turn_count),
                    format_number(row.session_count),
                    format_percent(row.call_share),
                    format_cost(row.estimated_cost_usd),
                )),
            ])
        }));
    }
    render_section(frame, area, "Behavior / Tools", lines);
}

fn render_optimize(
    frame: &mut Frame,
    area: Rect,
    payload: &OptimizePayload,
    zombie: &ZombieReport,
) {
    let mut lines = vec![support_line(&payload.support)];
    lines.push(Line::styled(
        "Read-only advice: llmusage never deletes, archives, rewrites, or cleans content automatically.",
        theme::muted_style(),
    ));

    if !payload.support.supported {
        if let Some(reason) = &payload.support.reason {
            lines.push(Line::styled(reason.clone(), theme::muted_style()));
        }
        lines.push(Line::styled(
            "No behavior facts; score and savings are not calculated.",
            theme::muted_style(),
        ));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                format!("Score {} ({}) ", payload.score, payload.grade),
                theme::bold_fg_style(theme::positive_fg()),
            ),
            Span::raw(format!(
                "estimated savings={} / {}",
                format_number(payload.estimated_savings_tokens),
                format_cost(payload.estimated_savings_usd),
            )),
        ]));

        if payload.findings.is_empty() {
            lines.push(Line::styled(
                "No obvious waste patterns found; review the surrounding context manually.",
                theme::muted_style(),
            ));
        } else {
            lines.extend(payload.findings.iter().take(3).map(|finding| {
                Line::from(vec![
                    Span::styled(
                        format!("[{}] {} ", finding.severity, finding.title),
                        theme::bold_fg_style(severity_color(&finding.severity)),
                    ),
                    Span::raw(format!(
                        "{}. Recommendation: {}",
                        finding.evidence, finding.recommendation
                    )),
                ])
            }));
        }
    }

    if zombie.zombies.is_empty() {
        lines.push(Line::styled(
            format!(
                "Zombie skills/MCPs: none ({} installed items scanned)",
                zombie.installed_total
            ),
            theme::muted_style(),
        ));
    } else {
        let preview = zombie
            .zombies
            .iter()
            .take(3)
            .map(|item| format!("{}:{}/{}", item.source, item.kind, item.name))
            .collect::<Vec<_>>()
            .join(" · ");
        lines.push(Line::from(vec![
            Span::styled(
                format!("Zombies {} ", zombie.zombies.len()),
                theme::bold_fg_style(theme::warning_fg()),
            ),
            Span::raw(format!(
                "Installed but never called ({} installed): {} (cleanup candidates; llmusage never deletes automatically)",
                zombie.installed_total, preview,
            )),
        ]));
    }

    render_section(frame, area, "Behavior / Optimize (read-only)", lines);
}

fn render_compare(frame: &mut Frame, area: Rect, payload: &ModelComparePayload) {
    let mut lines = vec![support_line(&payload.support)];
    if let Some(warning) = &payload.warning {
        lines.push(Line::from(vec![
            Span::styled("Warning ", theme::bold_fg_style(theme::warning_fg())),
            Span::raw(warning.clone()),
        ]));
    }

    match (&payload.model_a, &payload.model_b) {
        (Some(left), Some(right)) => {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} vs {} ", left.model, right.model),
                    theme::bold_fg_style(theme::accent()),
                ),
                Span::raw(format!(
                    "calls {}:{} / {}:{} edits {}:{} / {}:{} cost {}:{} / {}:{}",
                    left.model,
                    format_number(left.calls),
                    right.model,
                    format_number(right.calls),
                    left.model,
                    format_number(left.edit_turns),
                    right.model,
                    format_number(right.edit_turns),
                    left.model,
                    format_cost(left.estimated_cost_usd),
                    right.model,
                    format_cost(right.estimated_cost_usd),
                )),
            ]));
            lines.extend(payload.metrics.iter().take(3).map(|metric| {
                Line::from(vec![
                    Span::styled(
                        format!("{} ", metric.label),
                        theme::fg_style(theme::accent()),
                    ),
                    Span::raw(format!(
                        "{} → {}{}",
                        format_metric_value(metric.model_a_value),
                        format_metric_value(metric.model_b_value),
                        if metric.higher_is_better {
                            " (higher better)"
                        } else {
                            ""
                        }
                    )),
                ])
            }));
            if let Some(category) = payload.category_head_to_head.first() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("category:{} ", category.category),
                        theme::fg_style(theme::accent()),
                    ),
                    Span::raw(format!(
                        "one-shot {} vs {}",
                        format_percent(category.model_a_one_shot_rate),
                        format_percent(category.model_b_one_shot_rate),
                    )),
                ]));
            }
        }
        _ => {
            lines.push(empty_reason_line(
                &payload.support,
                "Compare requires at least two models with local usage.",
            ));
            lines.push(Line::styled(
                format!("Candidate models: {}", payload.candidates.len()),
                theme::muted_style(),
            ));
        }
    }

    render_section(frame, area, "Behavior / Model Comparison", lines);
}

fn render_section(frame: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'_>>) {
    let widget = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(section_block(title));
    frame.render_widget(widget, area);
}

fn section_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::fg_style(theme::border_normal()))
        .title(Span::styled(
            format!(" {title} "),
            theme::bold_fg_style(theme::accent()),
        ))
}

fn support_line(support: &BehaviorSupport) -> Line<'_> {
    let level = support_level_label(&support.level);
    let status_style = if support.supported {
        theme::bold_fg_style(theme::positive_fg())
    } else {
        theme::bold_fg_style(theme::warning_fg())
    };
    let mut spans = vec![
        Span::styled("Status ", theme::fg_style(theme::accent())),
        Span::styled(level, status_style),
    ];
    if let Some(reason) = &support.reason {
        spans.push(Span::raw(format!(" — {reason}")));
    }
    Line::from(spans)
}

fn empty_reason_line<'a>(support: &'a BehaviorSupport, fallback: &'a str) -> Line<'a> {
    Line::styled(
        support.reason.as_deref().unwrap_or(fallback),
        theme::muted_style(),
    )
}

fn support_level_label(level: &str) -> String {
    level.replace('_', "-")
}

fn severity_color(severity: &str) -> Color {
    match severity {
        "high" => theme::error_fg(),
        "medium" => theme::warning_fg(),
        _ => theme::positive_fg(),
    }
}
