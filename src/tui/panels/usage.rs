use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::common::i18n::{
    quota::{self as copy, Msg, label as q},
    text,
};
use crate::subscription::{
    UsageFetchDiagnostic, UsageFetchReport, UsageMetric, UsageOutput, UsageReadiness, output_score,
    readiness_status,
};
use crate::tui::{app::ScrollState, format::grouped as format_number, quota, theme};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    report: &Option<UsageFetchReport>,
    fetching: bool,
    hide_emails: bool,
    scroll: &ScrollState,
) {
    let title = if fetching {
        q(Msg::UsageSyncing)
    } else {
        q(Msg::Usage)
    };
    let block = theme::panel_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let content = render_action_bar(frame, inner, hide_emails, fetching);
    match report {
        None if fetching => render_message(frame, content, q(Msg::Fetching)),
        None => render_message(frame, content, q(Msg::NotLoaded)),
        Some(report) if report.outputs.is_empty() && fetching => {
            render_message(frame, content, q(Msg::Fetching))
        }
        Some(report) if report.outputs.is_empty() => render_empty(frame, content, report),
        Some(report) => render_loaded(frame, content, report, hide_emails, scroll),
    }
}

fn render_action_bar(frame: &mut Frame, area: Rect, hide_emails: bool, fetching: bool) -> Rect {
    let refresh = if fetching {
        q(Msg::RefreshSyncing)
    } else {
        q(Msg::Refresh)
    };
    let emails = if hide_emails {
        q(Msg::ShowEmails)
    } else {
        q(Msg::HideEmails)
    };
    let line = Line::from(vec![
        Span::styled(q(Msg::ActionsBar), theme::muted_style()),
        Span::styled(format!(" {refresh} "), action_style(true)),
        Span::raw(" "),
        Span::styled(format!(" {emails} "), action_style(false)),
        Span::raw(" "),
        Span::styled(q(Msg::SyncStatus), action_style(false)),
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
    let mut lines = vec![Line::from(Span::styled(q(Msg::Empty), theme::bold_style()))];
    if let Some(diagnostic) = report.diagnostics.first() {
        lines.push(Line::from(Span::styled(
            format!(
                "{}: {}",
                diagnostic.display_name(),
                diagnostic_message(diagnostic)
            ),
            theme::error_style(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            q(Msg::LoginHint),
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
    let block = theme::trend_card_block(q(Msg::Summary), theme::accent());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let outputs = &report.outputs;
    let mut lines = Vec::new();
    push_kv(
        &mut lines,
        q(Msg::State),
        overall_state_label(outputs),
        readiness_style(overall_readiness(outputs)),
    );
    push_kv(
        &mut lines,
        q(Msg::Active),
        &active_label(outputs, hide_emails),
        theme::bold_fg_style(theme::positive_fg()),
    );
    push_kv(
        &mut lines,
        q(Msg::Capacity),
        &capacity_label(outputs),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        q(Msg::Fallback),
        &fallback_label(outputs, hide_emails),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        q(Msg::NextReset),
        &next_reset_label(outputs),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        q(Msg::Action),
        overall_action(outputs),
        readiness_style(overall_readiness(outputs)),
    );

    if !report.diagnostics.is_empty() && lines.len() + 2 < inner.height as usize {
        lines.push(Line::from(""));
        lines.push(section(q(Msg::Diagnostics)));
        for diagnostic in report.diagnostics.iter().take(2) {
            lines.push(diagnostic_line(diagnostic, inner.width as usize));
        }
    }

    if lines.len() + 2 < inner.height as usize {
        lines.push(Line::from(""));
        lines.push(section(q(Msg::Attention)));
        let attention = attention_outputs(outputs);
        if attention.is_empty() {
            lines.push(Line::from(Span::styled(
                q(Msg::NoAttention),
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
        lines.push(section(q(Msg::Providers)));
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
                    text(
                        &format!("{count} managed · {ready} ready"),
                        &format!("{count} 个托管账号 · {ready} 个可用"),
                    )
                    .to_string(),
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
        "{}  {}",
        q(Msg::Selected),
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
        q(Msg::Status),
        &format!(
            "{} · {}",
            selected.plan.as_deref().unwrap_or(q(Msg::Unknown)),
            readiness_label(readiness_status(selected))
        ),
        readiness_style(readiness_status(selected)),
    );
    push_kv(
        &mut lines,
        q(Msg::Email),
        &email_display(selected.email.as_deref(), hide_emails),
        theme::muted_style(),
    );
    push_kv(
        &mut lines,
        q(Msg::Credential),
        &credential_label(selected),
        theme::muted_style(),
    );
    lines.push(section(q(Msg::Limits)));
    if selected.metrics.is_empty() {
        lines.push(Line::from(Span::styled(
            q(Msg::NoMetrics),
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
    lines.push(section(q(Msg::Actions)));
    lines.push(Line::from(Span::styled(
        q(Msg::ExternallyManagedIndented),
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
    let block = theme::trend_card_block(q(Msg::Accounts), theme::accent());
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
            q(Msg::NarrowHeader),
            theme::header_style(),
        ))]);
        let table = Table::new(rows, [Constraint::Percentage(100)]).header(header);
        frame.render_widget(table, inner);
        return;
    }

    let header = Row::new(
        [
            "#",
            q(Msg::Provider),
            q(Msg::Account),
            q(Msg::Plan),
            q(Msg::Auth),
            q(Msg::Health),
            q(Msg::Limit),
            q(Msg::Reset),
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
        Cell::from(
            output
                .plan
                .clone()
                .unwrap_or_else(|| q(Msg::Unknown).into()),
        ),
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
    let text = format!(
        "  {}: {}",
        diagnostic.display_name(),
        diagnostic_message(diagnostic)
    );
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
        quota::metric_hint(metric)
            .as_ref()
            .map(copy::metric_label)
            .unwrap_or_else(|| copy::original(&metric.label)),
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
    if let Some(hint) = quota::metric_hint(metric).and_then(|hint| hint.remaining) {
        return copy::absolute_left(&hint);
    }
    metric
        .remaining_label
        .as_deref()
        .map(copy::original)
        .unwrap_or_else(|| copy::percent_left(metric.remaining_percent))
}

fn diagnostic_message(diagnostic: &UsageFetchDiagnostic) -> String {
    quota::diagnostic_hint(diagnostic)
        .map(|hint| copy::diagnostic(&diagnostic.provider, &hint, &diagnostic.message))
        .unwrap_or_else(|| copy::original(&diagnostic.message))
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
        Some(_) if hide => q(Msg::HiddenEmail).to_string(),
        Some(email) => email.to_string(),
        None => "-".to_string(),
    }
}

fn credential_label(output: &UsageOutput) -> String {
    match output.credential_source.as_deref() {
        Some("opencode") => q(Msg::OpenCodeManaged).to_string(),
        _ => q(Msg::ExternallyManaged).to_string(),
    }
}

fn auth_label(output: &UsageOutput) -> String {
    match output.credential_source.as_deref() {
        Some("opencode") => "OpenCode".to_string(),
        _ => q(Msg::Managed).to_string(),
    }
}

fn readiness_label(status: UsageReadiness) -> &'static str {
    match status {
        UsageReadiness::Ready => q(Msg::Ready),
        UsageReadiness::Watch => q(Msg::Watch),
        UsageReadiness::Critical => q(Msg::QuotaLow),
        UsageReadiness::Unknown => q(Msg::Unknown),
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
        UsageReadiness::Ready => q(Msg::Ready),
        UsageReadiness::Watch => q(Msg::ReadyWarnings),
        UsageReadiness::Critical => q(Msg::QuotaLowState),
        UsageReadiness::Unknown => q(Msg::Unknown),
    }
}

fn overall_action(outputs: &[UsageOutput]) -> &'static str {
    if outputs.iter().any(|output| {
        output
            .account
            .as_ref()
            .is_some_and(|account| account.is_active)
    }) {
        q(Msg::StayActive)
    } else {
        q(Msg::ChooseActive)
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
    text(
        &format!("{ready} ready · {watch} watch · {critical} critical"),
        &format!("{ready} 个可用 · {watch} 个需关注 · {critical} 个额度偏低"),
    )
    .to_string()
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
        .unwrap_or_else(|| q(Msg::NoActive).to_string())
}

fn fallback_label(outputs: &[UsageOutput], hide_emails: bool) -> String {
    outputs
        .iter()
        .filter(|output| readiness_status(output) == UsageReadiness::Ready)
        .max_by(|left, right| output_score(left).total_cmp(&output_score(right)))
        .map(|output| {
            format!(
                "{} · {}",
                account_name(output, hide_emails),
                copy::percent_left(output_score(output))
            )
        })
        .unwrap_or_else(|| q(Msg::NoFallback).to_string())
}

fn next_reset_label(outputs: &[UsageOutput]) -> String {
    outputs
        .iter()
        .filter_map(display_metric)
        .filter_map(|metric| metric.resets_at.as_deref())
        .min()
        .map(format_reset_time)
        .unwrap_or_else(|| q(Msg::NoReset).to_string())
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
    text(
        &format!(
            "  Snapshot  {ready} ready · {risk} at risk · {} managed · emails hidden",
            report.outputs.len()
        ),
        &format!(
            "  快照  {ready} 个可用 · {risk} 个需关注 · {} 个托管账号 · 邮箱已隐藏",
            report.outputs.len()
        ),
    )
    .to_string()
}

pub fn status_label(report: &UsageFetchReport) -> String {
    let providers = unique_providers(&report.outputs).len();
    let issues = report.diagnostics.len();
    if issues == 0 {
        text(
            &format!("{providers} providers · {} managed", report.outputs.len()),
            &format!("{providers} 个服务商 · {} 个托管账号", report.outputs.len()),
        )
        .to_string()
    } else {
        text(
            &format!(
                "{providers} providers · {} managed · {issues} issues",
                report.outputs.len()
            ),
            &format!(
                "{providers} 个服务商 · {} 个托管账号 · {issues} 个问题",
                report.outputs.len()
            ),
        )
        .to_string()
    }
}

fn format_reset_time(value: &str) -> String {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_string();
    };
    let now = chrono::Utc::now();
    let diff = parsed.with_timezone(&chrono::Utc) - now;
    if diff.num_seconds() <= 0 {
        return q(Msg::ResetsNow).into();
    }
    let mins = diff.num_minutes();
    if mins < 60 {
        text(&format!("resets in {mins}m"), &format!("{mins} 分钟后重置")).to_string()
    } else if mins < 24 * 60 {
        text(
            &format!("resets in {}h", diff.num_hours()),
            &format!("{} 小时后重置", diff.num_hours()),
        )
        .to_string()
    } else {
        parsed
            .with_timezone(&chrono::Local)
            .format(text("resets %a %b %e %H:%M", "%m-%d %H:%M 重置"))
            .to_string()
    }
}

fn truncate(value: &str, max: usize) -> String {
    if UnicodeWidthStr::width(value) <= max {
        value.to_string()
    } else {
        let mut width = 0;
        let keep: String = value
            .chars()
            .take_while(|ch| {
                width += UnicodeWidthChar::width(*ch).unwrap_or(0);
                width < max
            })
            .collect();
        format!("{keep}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::i18n::{Language, with_language};
    use crate::subscription::{
        DiagnosticHint, MetricHint, QuotaPresentationHints, RemainingHint, WindowHint,
    };
    use ratatui::{Terminal, backend::TestBackend};

    fn fixture() -> (UsageFetchReport, QuotaPresentationHints) {
        let report = UsageFetchReport {
            outputs: vec![UsageOutput {
                provider: "Kimi".into(),
                account: None,
                credential_source: Some("opencode".into()),
                plan: Some("Pro".into()),
                email: Some("user@example.com".into()),
                metrics: vec![UsageMetric {
                    label: "Weekly".into(),
                    used_percent: 0.0,
                    remaining_percent: 100.0,
                    remaining_label: Some("101/100 left".into()),
                    resets_at: Some("2000-01-01T00:00:00Z".into()),
                }],
            }],
            diagnostics: vec![UsageFetchDiagnostic::error(
                "Claude",
                "Claude usage request failed (HTTP 429 Too Many Requests)",
            )],
        };
        let mut hints = QuotaPresentationHints::default();
        hints.metrics.push(vec![MetricHint {
            window: WindowHint::Weekly,
            prefix: None,
            remaining: Some(RemainingHint::Integer {
                remaining: 101,
                limit: 100,
            }),
        }]);
        hints
            .diagnostics
            .push(Some(DiagnosticHint::Http { status: 429 }));
        hints.bind(&report);
        (report, hints)
    }

    fn draw(
        report: Option<UsageFetchReport>,
        hints: Option<&QuotaPresentationHints>,
        language: Language,
        width: u16,
        height: u16,
        fetching: bool,
        hide: bool,
    ) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        with_language(language, || {
            quota::with_presentation(report.as_ref(), hints, || {
                terminal
                    .draw(|frame| {
                        render(
                            frame,
                            frame.area(),
                            &report,
                            fetching,
                            hide,
                            &ScrollState {
                                offset: 0,
                                selected: 0,
                                total: report.as_ref().map_or(0, |report| report.outputs.len()),
                                visible: height.saturating_sub(4) as usize,
                            },
                        )
                    })
                    .unwrap();
            })
        });
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..height {
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                text.push_str(symbol);
                x += UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn quota_all_layouts_localize_without_reconstructing_absolute_quantities() {
        let (report, hints) = fixture();
        let canonical = serde_json::to_value(&report).unwrap();
        for (width, height) in [(100, 32), (124, 38), (180, 50)] {
            let english = draw(
                Some(report.clone()),
                Some(&hints),
                Language::English,
                width,
                height,
                false,
                true,
            );
            let chinese = draw(
                Some(report.clone()),
                Some(&hints),
                Language::Chinese,
                width,
                height,
                false,
                true,
            );
            assert!(english.contains("Usage"), "{english}");
            assert!(chinese.contains("额度"), "{chinese}");
            assert!(chinese.contains("所选账号"), "{chinese}");
            assert!(chinese.contains("剩余 101/100"), "{chinese}");
            assert!(chinese.contains("每周"), "{chinese}");
            assert!(chinese.contains("[邮箱已隐藏]"), "{chinese}");
            assert!(!chinese.contains("user@example.com"));
            assert!(!chinese.contains("101/100 left"));
            assert!(
                draw(
                    Some(report.clone()),
                    Some(&hints),
                    Language::Chinese,
                    width,
                    height,
                    false,
                    false
                )
                .contains("user@example.com")
            );
        }
        assert_eq!(serde_json::to_value(report).unwrap(), canonical);
    }

    #[test]
    fn quota_loading_empty_diagnostics_and_legacy_detail_are_explicit() {
        assert!(
            draw(None, None, Language::Chinese, 100, 20, true, true).contains("正在获取订阅数据")
        );
        assert!(
            draw(None, None, Language::Chinese, 100, 20, false, true).contains("尚未加载订阅数据")
        );
        assert!(
            draw(
                Some(UsageFetchReport::default()),
                None,
                Language::Chinese,
                120,
                20,
                false,
                true
            )
            .contains("登录 Claude")
        );
        let (mut report, _) = fixture();
        report.outputs.clear();
        let mut hints = QuotaPresentationHints::default();
        hints
            .diagnostics
            .push(Some(DiagnosticHint::Http { status: 429 }));
        hints.bind(&report);
        let known = draw(
            Some(report.clone()),
            Some(&hints),
            Language::Chinese,
            120,
            20,
            false,
            true,
        );
        assert!(known.contains("额度请求失败（HTTP 429）"), "{known}");
        assert!(!known.contains("Original detail"));
        assert!(!known.contains("usage request failed"));
        assert!(
            draw(Some(report), None, Language::Chinese, 160, 20, false, true).contains("原始详情")
        );
        let (report, _) = fixture();
        with_language(Language::Chinese, || {
            assert_eq!(
                remaining_label(&report.outputs[0].metrics[0]),
                "原始详情: 101/100 left"
            );
            assert_eq!(format_reset_time("2000-01-01T00:00:00Z"), "即将重置");
            assert_eq!(truncate("中文abc", 5), "中文…");
        });
    }

    #[test]
    fn quota_chinese_preserves_nocolor_and_selection_styles() {
        let previous = theme::color_mode();
        struct Restore(theme::TerminalColorMode);
        impl Drop for Restore {
            fn drop(&mut self) {
                theme::set_color_mode(self.0);
            }
        }
        let _restore = Restore(previous);
        theme::set_color_mode(theme::TerminalColorMode::NoColor);
        let (report, hints) = fixture();
        let report = Some(report);
        let mut terminal = Terminal::new(TestBackend::new(180, 50)).unwrap();
        with_language(Language::Chinese, || {
            quota::with_presentation(report.as_ref(), Some(&hints), || {
                theme::with_render_snapshot(|| {
                    terminal
                        .draw(|frame| {
                            render(
                                frame,
                                frame.area(),
                                &report,
                                false,
                                true,
                                &ScrollState {
                                    offset: 0,
                                    selected: 0,
                                    total: 1,
                                    visible: 20,
                                },
                            )
                        })
                        .unwrap()
                });
            })
        });
        for cell in &terminal.backend().buffer().content {
            assert_eq!(cell.fg, ratatui::buffer::Cell::default().fg);
            assert_eq!(cell.bg, ratatui::buffer::Cell::default().bg);
            assert!(cell.modifier.is_empty());
        }
    }
}
