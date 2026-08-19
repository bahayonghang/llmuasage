use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::subscription::{
    UsageFetchDiagnostic, UsageFetchReport, UsageMetric, UsageOutput, UsageReadiness, output_score,
    readiness_status,
};
use crate::tui::{app::ScrollState, format::grouped as format_number, theme};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    report: &Option<UsageFetchReport>,
    fetching: bool,
    hide_emails: bool,
    scroll: &ScrollState,
) {
    let title = if fetching { "Usage  Syncing" } else { "Usage" };
    let block = theme::panel_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let content = render_action_bar(frame, inner, hide_emails, fetching);
    match report {
        None if fetching => render_message(frame, content, "Fetching subscription data..."),
        None => render_message(frame, content, "No subscription data loaded"),
        Some(report) if report.outputs.is_empty() && fetching => {
            render_message(frame, content, "Fetching subscription data...")
        }
        Some(report) if report.outputs.is_empty() => render_empty(frame, content, report),
        Some(report) => render_loaded(frame, content, report, hide_emails, scroll),
    }
}

fn render_action_bar(frame: &mut Frame, area: Rect, hide_emails: bool, fetching: bool) -> Rect {
    let refresh = if fetching { "r Syncing" } else { "r Refresh" };
    let emails = if hide_emails {
        "m Show Emails"
    } else {
        "m Hide Emails"
    };
    let line = Line::from(vec![
        Span::styled(" Actions ", theme::muted_style()),
        Span::styled(format!(" {refresh} "), action_style(true)),
        Span::raw(" "),
        Span::styled(format!(" {emails} "), action_style(false)),
        Span::raw(" "),
        Span::styled(" y Sync status ", action_style(false)),
    ]);
    frame.render_widget(
        Paragraph::new(line),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height > 1 {
        Rect::new(area.x, area.y + 1, area.width, area.height - 1)
    } else {
        Rect::new(area.x, area.y, area.width, 0)
    }
}

fn action_style(primary: bool) -> Style {
    if primary {
        theme::selection_style()
    } else {
        theme::bold_fg_style(theme::accent())
    }
}

fn render_message(frame: &mut Frame, area: Rect, message: &str) {
    frame.render_widget(
        Paragraph::new(message)
            .style(theme::muted_style())
            .alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

fn render_empty(frame: &mut Frame, area: Rect, report: &UsageFetchReport) {
    let mut lines = vec![Line::from(Span::styled(
        "No subscription data available",
        theme::bold_style(),
    ))];
    if let Some(diagnostic) = report.diagnostics.first() {
        lines.push(Line::from(Span::styled(
            format!("{}: {}", diagnostic.display_name(), diagnostic.message),
            theme::error_style(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Use Refresh after logging into Claude, Codex, Grok, or Kimi.",
            theme::muted_style(),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

fn render_loaded(
    frame: &mut Frame,
    area: Rect,
    report: &UsageFetchReport,
    hide_emails: bool,
    scroll: &ScrollState,
) {
    let outputs = &report.outputs;
    let selected = scroll.selected.min(outputs.len().saturating_sub(1));
    if area.width < 104 || area.height < 20 {
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(7.min(area.height))])
            .split(area);
        render_accounts_table(frame, chunks[0], outputs, scroll, hide_emails);
        if chunks.len() > 1 {
            render_selected_account(frame, chunks[1], &outputs[selected], report, hide_emails);
        }
        return;
    }

    if area.width < 132 {
        let chunks = Layout::vertical([
            Constraint::Length(8.min(area.height)),
            Constraint::Length(9.min(area.height.saturating_sub(8))),
            Constraint::Min(0),
        ])
        .split(area);
        render_summary(frame, chunks[0], report, hide_emails);
        render_selected_account(frame, chunks[1], &outputs[selected], report, hide_emails);
        render_accounts_table(frame, chunks[2], outputs, scroll, hide_emails);
        return;
    }

    let top_height = (area.height / 2).clamp(9, 19);
    let chunks = Layout::vertical([Constraint::Length(top_height), Constraint::Min(0)]).split(area);
    let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);
    render_summary(frame, top[0], report, hide_emails);
    render_selected_account(frame, top[1], &outputs[selected], report, hide_emails);
    render_accounts_table(frame, chunks[1], outputs, scroll, hide_emails);
}

fn render_summary(frame: &mut Frame, area: Rect, report: &UsageFetchReport, hide_emails: bool) {
    let block = theme::trend_card_block("Usage Summary", theme::accent());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let outputs = &report.outputs;
    let mut lines = Vec::new();
    push_kv(
        &mut lines,
        "State",
        overall_state_label(outputs),
        readiness_style(overall_readiness(outputs)),
    );
    push_kv(
        &mut lines,
        "Active",
        &active_label(outputs, hide_emails),
        theme::bold_fg_style(theme::positive_fg()),
    );
    push_kv(
        &mut lines,
        "Capacity",
        &capacity_label(outputs),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        "Fallback",
        &fallback_label(outputs, hide_emails),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        "Next Reset",
        &next_reset_label(outputs),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        "Action",
        overall_action(outputs),
        readiness_style(overall_readiness(outputs)),
    );

    if !report.diagnostics.is_empty() && lines.len() + 2 < inner.height as usize {
        lines.push(Line::from(""));
        lines.push(section("Diagnostics"));
        for diagnostic in report.diagnostics.iter().take(2) {
            lines.push(diagnostic_line(diagnostic, inner.width as usize));
        }
    }

    if lines.len() + 2 < inner.height as usize {
        lines.push(Line::from(""));
        lines.push(section("Attention"));
        let attention = attention_outputs(outputs);
        if attention.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No accounts need attention",
                theme::muted_style(),
            )));
        } else {
            for output in attention.into_iter().take(2) {
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {:<11} {}",
                        readiness_label(readiness_status(output)),
                        account_name(output, hide_emails)
                    ),
                    readiness_style(readiness_status(output)),
                )));
            }
        }
    }

    if lines.len() + 2 < inner.height as usize {
        lines.push(Line::from(""));
        lines.push(section("Providers"));
        for provider in unique_providers(outputs) {
            let count = outputs
                .iter()
                .filter(|output| output.provider == provider)
                .count();
            let ready = outputs
                .iter()
                .filter(|output| {
                    output.provider == provider && readiness_status(output) == UsageReadiness::Ready
                })
                .count();
            lines.push(Line::from(vec![
                Span::styled(format!("  {provider:<16}"), theme::bold_style()),
                Span::styled(
                    format!("{count} managed · {ready} ready"),
                    theme::muted_style(),
                ),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_selected_account(
    frame: &mut Frame,
    area: Rect,
    selected: &UsageOutput,
    report: &UsageFetchReport,
    hide_emails: bool,
) {
    let title = format!(
        "Selected Account  {}",
        output_display_name(selected, hide_emails)
    );
    let block = theme::trend_card_block(&title, theme::accent());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = Vec::new();
    push_kv(
        &mut lines,
        "Status",
        &format!(
            "{} · {}",
            selected.plan.as_deref().unwrap_or("Unknown"),
            readiness_label(readiness_status(selected))
        ),
        readiness_style(readiness_status(selected)),
    );
    push_kv(
        &mut lines,
        "Email",
        &email_display(selected.email.as_deref(), hide_emails),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        "Credential",
        &credential_label(selected),
        theme::muted_style(),
    );
    lines.push(section("Limits"));
    if selected.metrics.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No quota metrics returned",
            theme::muted_style(),
        )));
    } else {
        for metric in selected.metrics.iter().take(4) {
            lines.push(metric_line(metric, inner.width as usize));
        }
    }
    lines.push(Line::from(Span::styled(
        snapshot_label(report),
        theme::muted_style(),
    )));
    lines.push(section("Actions"));
    lines.push(Line::from(Span::styled(
        "  Managed externally",
        theme::muted_style(),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_accounts_table(
    frame: &mut Frame,
    area: Rect,
    outputs: &[UsageOutput],
    scroll: &ScrollState,
    hide_emails: bool,
) {
    let block = theme::trend_card_block("Accounts", theme::accent());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let visible = super::visible_table_rows(inner);
    let start = scroll
        .offset
        .min(outputs.len().saturating_sub(visible.min(outputs.len())));
    let rows = outputs
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(index, output)| account_row(output, index, scroll.selected == index, hide_emails));

    if inner.width < 88 {
        let header = Row::new([Cell::from(Span::styled(
            " #  Account / Status",
            theme::header_style(),
        ))]);
        let table = Table::new(rows, [Constraint::Percentage(100)]).header(header);
        frame.render_widget(table, inner);
        return;
    }

    let header = Row::new(
        [
            "#", "Provider", "Account", "Plan", "Auth", "Health", "Limit", "Reset",
        ]
        .iter()
        .map(|label| Cell::from(Span::styled(*label, theme::header_style()))),
    )
    .bottom_margin(1);
    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Length(12),
            Constraint::Min(16),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Min(18),
            Constraint::Length(18),
        ],
    )
    .header(header);
    frame.render_widget(table, inner);
}

fn account_row(
    output: &UsageOutput,
    index: usize,
    selected: bool,
    hide_emails: bool,
) -> Row<'static> {
    let readiness = readiness_status(output);
    let metric = display_metric(output);
    let limit = metric
        .map(remaining_label)
        .unwrap_or_else(|| "-".to_string());
    let reset = metric
        .and_then(|metric| metric.resets_at.as_deref())
        .map(format_reset_time)
        .unwrap_or_else(|| "-".to_string());
    let mut row = Row::new(vec![
        Cell::from(format_number(index as i64 + 1)),
        Cell::from(output.provider.clone()).style(theme::bold_style()),
        Cell::from(account_name(output, hide_emails)),
        Cell::from(output.plan.clone().unwrap_or_else(|| "Unknown".into())),
        Cell::from(auth_label(output)),
        Cell::from(readiness_label(readiness)).style(readiness_style(readiness)),
        Cell::from(limit),
        Cell::from(reset),
    ]);
    if selected {
        row = row.style(theme::selection_style());
    } else if index % 2 == 1 {
        row = row.style(theme::row_alt_style());
    }
    row
}

fn push_kv(lines: &mut Vec<Line<'static>>, key: &str, value: &str, style: Style) {
    lines.push(Line::from(vec![
        Span::styled(format!("  {key:<12}"), theme::muted_style()),
        Span::styled(value.to_string(), style),
    ]));
}

fn section(label: &str) -> Line<'static> {
    Line::from(Span::styled(format!("  {label}"), theme::bold_style()))
}

fn diagnostic_line(diagnostic: &UsageFetchDiagnostic, width: usize) -> Line<'static> {
    let text = format!("  {}: {}", diagnostic.display_name(), diagnostic.message);
    Line::from(Span::styled(truncate(&text, width), theme::error_style()))
}

fn metric_line(metric: &UsageMetric, width: usize) -> Line<'static> {
    let bar = ratio_bar(metric.remaining_percent, 12);
    let reset = metric
        .resets_at
        .as_deref()
        .map(format_reset_time)
        .unwrap_or_default();
    let text = format!(
        "  {:<10} {} {:>8}  {reset}",
        metric.label,
        bar,
        remaining_label(metric)
    );
    Line::from(Span::styled(
        truncate(&text, width),
        theme::fg_style(theme::bar_color(metric.used_percent)),
    ))
}

fn ratio_bar(remaining: f64, width: usize) -> String {
    let filled = ((remaining.clamp(0.0, 100.0) / 100.0) * width as f64).round() as usize;
    format!(
        "[{}{}]",
        "=".repeat(filled),
        "-".repeat(width.saturating_sub(filled))
    )
}

fn remaining_label(metric: &UsageMetric) -> String {
    metric
        .remaining_label
        .clone()
        .unwrap_or_else(|| format!("{:.0}% left", metric.remaining_percent))
}

fn display_metric(output: &UsageOutput) -> Option<&UsageMetric> {
    output.metrics.first()
}

fn account_name(output: &UsageOutput, hide_emails: bool) -> String {
    if let Some(account) = &output.account
        && let Some(label) = account
            .label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        return label.to_string();
    }
    email_display(output.email.as_deref(), hide_emails)
}

fn output_display_name(output: &UsageOutput, hide_emails: bool) -> String {
    format!(
        "{} ({})",
        output.provider,
        account_name(output, hide_emails)
    )
}

fn email_display(email: Option<&str>, hide: bool) -> String {
    match email.map(str::trim).filter(|value| !value.is_empty()) {
        Some(_) if hide => "[hidden email]".to_string(),
        Some(email) => email.to_string(),
        None => "-".to_string(),
    }
}

fn credential_label(output: &UsageOutput) -> String {
    match output.credential_source.as_deref() {
        Some("opencode") => "managed by OpenCode".to_string(),
        _ => "managed externally".to_string(),
    }
}

fn auth_label(output: &UsageOutput) -> String {
    match output.credential_source.as_deref() {
        Some("opencode") => "OpenCode".to_string(),
        _ => "Managed".to_string(),
    }
}

fn readiness_label(status: UsageReadiness) -> &'static str {
    match status {
        UsageReadiness::Ready => "Ready",
        UsageReadiness::Watch => "Watch",
        UsageReadiness::Critical => "Quota Low",
        UsageReadiness::Unknown => "Unknown",
    }
}

fn readiness_style(status: UsageReadiness) -> Style {
    match status {
        UsageReadiness::Ready => theme::bold_fg_style(theme::positive_fg()),
        UsageReadiness::Watch => theme::bold_fg_style(theme::warning_fg()),
        UsageReadiness::Critical => theme::error_style(),
        UsageReadiness::Unknown => theme::muted_style(),
    }
}

fn overall_readiness(outputs: &[UsageOutput]) -> UsageReadiness {
    if outputs
        .iter()
        .any(|output| readiness_status(output) == UsageReadiness::Critical)
    {
        UsageReadiness::Critical
    } else if outputs
        .iter()
        .any(|output| readiness_status(output).is_at_risk())
    {
        UsageReadiness::Watch
    } else if outputs
        .iter()
        .any(|output| readiness_status(output) == UsageReadiness::Ready)
    {
        UsageReadiness::Ready
    } else {
        UsageReadiness::Unknown
    }
}

fn overall_state_label(outputs: &[UsageOutput]) -> &'static str {
    match overall_readiness(outputs) {
        UsageReadiness::Ready => "Ready",
        UsageReadiness::Watch => "Ready with warnings",
        UsageReadiness::Critical => "Quota low",
        UsageReadiness::Unknown => "Unknown",
    }
}

fn overall_action(outputs: &[UsageOutput]) -> &'static str {
    if outputs.iter().any(|output| {
        output
            .account
            .as_ref()
            .is_some_and(|account| account.is_active)
    }) {
        "Stay on the active account"
    } else {
        "Choose an active account"
    }
}

fn capacity_label(outputs: &[UsageOutput]) -> String {
    let ready = outputs
        .iter()
        .filter(|output| readiness_status(output) == UsageReadiness::Ready)
        .count();
    let watch = outputs
        .iter()
        .filter(|output| readiness_status(output) == UsageReadiness::Watch)
        .count();
    let critical = outputs
        .iter()
        .filter(|output| readiness_status(output) == UsageReadiness::Critical)
        .count();
    format!("{ready} ready · {watch} watch · {critical} critical")
}

fn active_label(outputs: &[UsageOutput], hide_emails: bool) -> String {
    outputs
        .iter()
        .find(|output| {
            output
                .account
                .as_ref()
                .is_some_and(|account| account.is_active)
        })
        .or_else(|| {
            outputs
                .iter()
                .find(|output| readiness_status(output) == UsageReadiness::Ready)
        })
        .map(|output| account_name(output, hide_emails))
        .unwrap_or_else(|| "No active account".to_string())
}

fn fallback_label(outputs: &[UsageOutput], hide_emails: bool) -> String {
    outputs
        .iter()
        .filter(|output| readiness_status(output) == UsageReadiness::Ready)
        .max_by(|left, right| output_score(left).total_cmp(&output_score(right)))
        .map(|output| {
            format!(
                "{} · {:.0}% left",
                account_name(output, hide_emails),
                output_score(output)
            )
        })
        .unwrap_or_else(|| "No ready fallback".to_string())
}

fn next_reset_label(outputs: &[UsageOutput]) -> String {
    outputs
        .iter()
        .filter_map(display_metric)
        .filter_map(|metric| metric.resets_at.as_deref())
        .min()
        .map(format_reset_time)
        .unwrap_or_else(|| "No reset data".to_string())
}

fn attention_outputs(outputs: &[UsageOutput]) -> Vec<&UsageOutput> {
    outputs
        .iter()
        .filter(|output| readiness_status(output).is_at_risk())
        .collect()
}

fn unique_providers(outputs: &[UsageOutput]) -> Vec<String> {
    let mut providers = Vec::new();
    for output in outputs {
        if !providers
            .iter()
            .any(|provider| provider == &output.provider)
        {
            providers.push(output.provider.clone());
        }
    }
    providers
}

fn snapshot_label(report: &UsageFetchReport) -> String {
    let ready = report
        .outputs
        .iter()
        .filter(|output| readiness_status(output) == UsageReadiness::Ready)
        .count();
    let risk = report
        .outputs
        .iter()
        .filter(|output| readiness_status(output).is_at_risk())
        .count();
    format!(
        "  Snapshot  {ready} ready · {risk} at risk · {} managed · emails hidden",
        report.outputs.len()
    )
}

pub fn status_label(report: &UsageFetchReport) -> String {
    let providers = unique_providers(&report.outputs).len();
    let issues = report.diagnostics.len();
    if issues == 0 {
        format!("{providers} providers · {} managed", report.outputs.len())
    } else {
        format!(
            "{providers} providers · {} managed · {issues} issues",
            report.outputs.len()
        )
    }
}

fn format_reset_time(value: &str) -> String {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_string();
    };
    let now = chrono::Utc::now();
    let diff = parsed.with_timezone(&chrono::Utc) - now;
    if diff.num_seconds() <= 0 {
        return "resets now".into();
    }
    let mins = diff.num_minutes();
    if mins < 60 {
        format!("resets in {mins}m")
    } else if mins < 24 * 60 {
        format!("resets in {}h", diff.num_hours())
    } else {
        parsed
            .with_timezone(&chrono::Local)
            .format("resets %a %b %e %H:%M")
            .to_string()
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let keep: String = value.chars().take(max.saturating_sub(1)).collect();
        format!("{keep}…")
    }
}
