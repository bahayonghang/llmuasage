use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::query::{BehaviorSupport, ModelComparePayload, OptimizePayload, ZombieReport};
use crate::tui::{app::BehaviorPanelPayload, theme};

/// Render the terminal behavior analytics summary.
pub fn render(frame: &mut Frame, area: Rect, data: &Option<Result<BehaviorPanelPayload, String>>) {
    match data {
        None => {
            let widget = Paragraph::new("加载中...")
                .style(theme::muted_style())
                .block(theme::panel_block("行为"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("数据加载失败: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("行为"));
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
            "无 Activity 行为事实。",
        ));
    } else {
        lines.extend(payload.activity.breakdown.iter().take(4).map(|row| {
            Line::from(vec![
                Span::styled(
                    format!("{} ", row.category),
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::BOLD),
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
    render_section(frame, area, "行为 · Activity 分类", lines);
}

fn render_tools(frame: &mut Frame, area: Rect, payload: &BehaviorPanelPayload) {
    let mut lines = vec![support_line(&payload.tools.support)];
    if payload.tools.breakdown.is_empty() {
        lines.push(empty_reason_line(
            &payload.tools.support,
            "无 Tools 行为事实。",
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
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::BOLD),
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
    render_section(frame, area, "行为 · Tools 工具", lines);
}

fn render_optimize(
    frame: &mut Frame,
    area: Rect,
    payload: &OptimizePayload,
    zombie: &ZombieReport,
) {
    let mut lines = vec![support_line(&payload.support)];
    lines.push(Line::styled(
        "只读建议：llmusage 不会自动删除、归档、重写或清理任何内容。",
        theme::muted_style(),
    ));

    if !payload.support.supported {
        if let Some(reason) = &payload.support.reason {
            lines.push(Line::styled(reason.clone(), theme::muted_style()));
        }
        lines.push(Line::styled(
            "无行为事实，暂不计算 score 或 savings。",
            theme::muted_style(),
        ));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                format!("Score {} ({}) ", payload.score, payload.grade),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "estimated savings={} / {}",
                format_number(payload.estimated_savings_tokens),
                format_cost(payload.estimated_savings_usd),
            )),
        ]));

        if payload.findings.is_empty() {
            lines.push(Line::styled(
                "未发现明显浪费模式；继续结合上下文人工判断。",
                theme::muted_style(),
            ));
        } else {
            lines.extend(payload.findings.iter().take(3).map(|finding| {
                Line::from(vec![
                    Span::styled(
                        format!("[{}] {} ", finding.severity, finding.title),
                        Style::default()
                            .fg(severity_color(&finding.severity))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!(
                        "{}；建议：{}",
                        finding.evidence, finding.recommendation
                    )),
                ])
            }));
        }
    }

    if zombie.zombies.is_empty() {
        lines.push(Line::styled(
            format!(
                "僵尸技能/MCP：无（已扫描 {} 个已装项）",
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
                format!("僵尸 {} ", zombie.zombies.len()),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "装了从未调用（共 {} 已装）：{}（可清理候选；llmusage 不自动删除）",
                zombie.installed_total, preview,
            )),
        ]));
    }

    render_section(frame, area, "行为 · Optimize 只读建议", lines);
}

fn render_compare(frame: &mut Frame, area: Rect, payload: &ModelComparePayload) {
    let mut lines = vec![support_line(&payload.support)];
    if let Some(warning) = &payload.warning {
        lines.push(Line::from(vec![
            Span::styled(
                "警告 ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(warning.clone()),
        ]));
    }

    match (&payload.model_a, &payload.model_b) {
        (Some(left), Some(right)) => {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} vs {} ", left.model, right.model),
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::BOLD),
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
                        Style::default().fg(theme::accent()),
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
                        Style::default().fg(theme::accent()),
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
                "Compare 需要至少两个有本地用量的模型。",
            ));
            lines.push(Line::styled(
                format!("候选模型: {}", payload.candidates.len()),
                theme::muted_style(),
            ));
        }
    }

    render_section(frame, area, "行为 · Compare 模型对比", lines);
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
        .border_style(Style::default().fg(theme::border_normal()))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ))
}

fn support_line(support: &BehaviorSupport) -> Line<'_> {
    let level = support_level_label(&support.level);
    let status_style = if support.supported {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    };
    let mut spans = vec![
        Span::styled("状态 ", Style::default().fg(theme::accent())),
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
        "high" => Color::Red,
        "medium" => Color::Yellow,
        _ => Color::Green,
    }
}

fn format_cost(cost: f64) -> String {
    format!("${cost:.2}")
}

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn format_metric_value(value: f64) -> String {
    if (0.0..=1.0).contains(&value) {
        format_percent(value)
    } else {
        format!("{value:.2}")
    }
}

fn format_number(n: i64) -> String {
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
