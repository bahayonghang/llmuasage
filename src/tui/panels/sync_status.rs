use crate::common::i18n::tui::format as tr_format;
use crate::common::i18n::tui::text as tr;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    domain::platform_monitor::{ParserSupportStatus, PlatformProbe, PlatformProbeStatus},
    query::{SyncCommandCenterPayload, SyncLastRunPayload, SyncSourcePayload},
    tui::{format::grouped as format_number, theme},
};

use super::super::app::ScrollState;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<SyncCommandCenterPayload, String>>,
    probes: &[PlatformProbe],
    scroll: &ScrollState,
) {
    match data {
        None => {
            let widget = Paragraph::new(tr("loading"))
                .style(theme::muted_style())
                .block(theme::panel_block(tr("usage_sync")));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(tr_format!(data_load_failed_e, e = e))
                .style(theme::error_style())
                .block(theme::panel_block(tr("usage_sync")));
            frame.render_widget(widget, area);
        }
        Some(Ok(payload)) => render_payload(frame, area, payload, probes, scroll),
    }
}

fn render_payload(
    frame: &mut Frame,
    area: Rect,
    payload: &SyncCommandCenterPayload,
    probes: &[PlatformProbe],
    scroll: &ScrollState,
) {
    let block = theme::panel_block(tr("usage_sync"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let show_monitor = inner.height >= 16;
    let constraints = if show_monitor {
        vec![
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(5),
        ]
    } else {
        vec![Constraint::Length(5), Constraint::Min(5)]
    };
    let chunks = Layout::vertical(constraints).split(inner);

    render_summary(frame, chunks[0], payload, probes);
    render_sources(frame, chunks[1], payload, scroll);
    if show_monitor {
        render_monitor(frame, chunks[2], probes);
    }
}

fn render_summary(
    frame: &mut Frame,
    area: Rect,
    payload: &SyncCommandCenterPayload,
    probes: &[PlatformProbe],
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let status_style = tone_style(&payload.tone);
    let detected = probes
        .iter()
        .filter(|probe| probe.status == PlatformProbeStatus::Detected)
        .count();
    let monitor_only = probes
        .iter()
        .filter(|probe| probe.source_kind.is_none())
        .count();
    let source_total = payload.metrics.sources_total.max(0);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(headline_text(&payload.headline_key), status_style),
            Span::styled("  ", theme::muted_style()),
            Span::styled(reason_text(&payload.reason_key), theme::muted_style()),
        ]),
        Line::from(vec![
            Span::styled(tr("events_3"), theme::muted_style()),
            Span::styled(
                format_number(payload.metrics.events_seen),
                metric_style(theme::metric_input()),
            ),
            Span::styled(tr("inserted"), theme::muted_style()),
            Span::styled(
                format_number(payload.metrics.inserted_delta),
                metric_style(theme::positive_fg()),
            ),
            Span::styled(tr("stored"), theme::muted_style()),
            Span::styled(
                format_number(payload.metrics.stored_events),
                metric_style(theme::warning_fg()),
            ),
            Span::styled(tr("sources_2"), theme::muted_style()),
            Span::styled(
                format!("{}/{}", payload.metrics.sources_ready, source_total),
                metric_style(theme::metric_cache_write()),
            ),
        ]),
        Line::from(vec![
            Span::styled(tr("lock"), theme::muted_style()),
            Span::styled(
                crate::common::i18n::tui::value(&payload.safety.worker_lock),
                if payload.safety.worker_lock == "available" {
                    metric_style(theme::positive_fg())
                } else {
                    metric_style(theme::warning_fg())
                },
            ),
            Span::styled(tr("rebuild_risk"), theme::muted_style()),
            Span::styled(
                yes_no(payload.safety.lossy_rebuild_risk),
                theme::muted_style(),
            ),
            Span::styled(tr("monitored"), theme::muted_style()),
            Span::styled(
                tr_format!(detected_detected, probes.len(), detected = detected),
                metric_style(theme::metric_input()),
            ),
            Span::styled(tr("parserless"), theme::muted_style()),
            Span::styled(monitor_only.to_string(), metric_style(theme::warning_fg())),
        ]),
    ];

    if let Some(last_run) = &payload.last_run {
        lines.push(Line::from(last_run_spans(last_run)));
    } else {
        lines.push(Line::styled(
            tr("last_run_none_recorded"),
            theme::muted_style(),
        ));
    }
    if !payload.safety.risk_details.is_empty() {
        let details = payload
            .safety
            .risk_details
            .iter()
            .map(|detail| {
                tr_format!(
                    missing_protected,
                    detail.source,
                    detail.missing_file_count,
                    detail.protected_event_count
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(Line::from(vec![
            Span::styled(tr("rebuild_risk_facts"), theme::muted_style()),
            Span::styled(details, theme::muted_style()),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_sources(
    frame: &mut Frame,
    area: Rect,
    payload: &SyncCommandCenterPayload,
    scroll: &ScrollState,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if payload.sources.is_empty() {
        let empty = Paragraph::new(tr(
            "no_source_sync_status_yet_run_sync_or_refresh_after_imports",
        ))
        .style(theme::muted_style())
        .block(theme::trend_card_block(
            tr("source_sync"),
            theme::metric_input(),
        ));
        frame.render_widget(empty, area);
        return;
    }

    let very_narrow = area.width < 58;
    let narrow = area.width < 88;
    let visible_height = super::visible_table_rows(area);
    let rows = payload
        .sources
        .iter()
        .skip(scroll.offset)
        .take(visible_height)
        .enumerate()
        .map(|(index, source)| source_row(source, index, very_narrow, narrow));

    let header = Row::new(source_header(very_narrow, narrow))
        .style(theme::header_style())
        .bottom_margin(1);
    let table = Table::new(rows, source_widths(very_narrow, narrow))
        .header(header)
        .block(theme::trend_card_block(
            tr("source_sync"),
            theme::metric_input(),
        ));
    frame.render_widget(table, area);
}

fn render_monitor(frame: &mut Frame, area: Rect, probes: &[PlatformProbe]) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let detected = probes
        .iter()
        .filter(|probe| probe.status == PlatformProbeStatus::Detected)
        .count();
    let parserless_blocked = probes
        .iter()
        .filter(|probe| {
            probe.source_kind.is_none()
                && probe.parser_status == ParserSupportStatus::BlockedNoSamples
        })
        .count();
    let detected_names = probes
        .iter()
        .filter(|probe| probe.source_kind.is_none())
        .filter(|probe| probe.status == PlatformProbeStatus::Detected)
        .take(4)
        .map(|probe| probe.display_name)
        .collect::<Vec<_>>();
    let detected_line = if detected_names.is_empty() {
        tr("monitor_only_detected_none").to_string()
    } else {
        tr_format!(monitor_only_detected, detected_names.join(", "))
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(tr("platform_probes"), theme::muted_style()),
            Span::styled(
                format!("{detected}/{}", probes.len()),
                metric_style(theme::metric_input()),
            ),
            Span::styled(tr("blocked_no_samples"), theme::muted_style()),
            Span::styled(
                parserless_blocked.to_string(),
                metric_style(theme::warning_fg()),
            ),
        ]),
        Line::styled(detected_line, theme::muted_style()),
        Line::styled(
            tr("parserless_platforms_stay_monitor_only_until_fixtures_and_token_semantics_exist"),
            theme::muted_style(),
        ),
    ];
    let widget = Paragraph::new(lines).block(theme::trend_card_block(
        tr("platform_monitor"),
        theme::metric_cache_write(),
    ));
    frame.render_widget(widget, area);
}

fn source_row(
    source: &SyncSourcePayload,
    index: usize,
    very_narrow: bool,
    narrow: bool,
) -> Row<'static> {
    let mut row = if very_narrow {
        Row::new(vec![
            Cell::from(source.source.clone()),
            Cell::from(crate::common::i18n::tui::value(&source.status).to_string())
                .style(tone_style(&source.tone)),
            Cell::from(format_number(source.stored_events)),
        ])
    } else if narrow {
        Row::new(vec![
            Cell::from(source.source.clone()),
            Cell::from(crate::common::i18n::tui::value(&source.status).to_string())
                .style(tone_style(&source.tone)),
            Cell::from(format_number(source.events_seen)),
            Cell::from(format_number(source.stored_events)),
        ])
    } else {
        Row::new(vec![
            Cell::from(source.source.clone()).style(theme::bold_style()),
            Cell::from(crate::common::i18n::tui::value(&source.status).to_string())
                .style(tone_style(&source.tone)),
            Cell::from(format_number(source.events_seen)),
            Cell::from(format_number(source.events_inserted)),
            Cell::from(format_number(source.skipped_files)),
            Cell::from(format_number(source.stored_events)),
            Cell::from(Line::from(issue_spans(source))),
            Cell::from(format!("{:.0}%", source.share * 100.0)),
            Cell::from(source.updated_at.clone().unwrap_or_else(|| "-".to_string())),
        ])
    };

    if index % 2 == 1 {
        row = row.style(theme::row_alt_style());
    }
    row
}

fn source_header(very_narrow: bool, narrow: bool) -> Vec<Cell<'static>> {
    let labels: &[&str] = if very_narrow {
        &[tr("source_3"), tr("status_2"), tr("stored_2")]
    } else if narrow {
        &[tr("source_3"), tr("status_2"), tr("seen"), tr("stored_2")]
    } else {
        &[
            tr("source_3"),
            tr("status_2"),
            tr("seen"),
            tr("inserted_2"),
            tr("skipped"),
            tr("stored_2"),
            tr("issues"),
            tr("share"),
            tr("updated"),
        ]
    };
    labels
        .iter()
        .map(|label| Cell::from(Span::styled(*label, theme::header_style())))
        .collect()
}

fn source_widths(very_narrow: bool, narrow: bool) -> Vec<Constraint> {
    if very_narrow {
        vec![
            Constraint::Percentage(36),
            Constraint::Percentage(32),
            Constraint::Percentage(32),
        ]
    } else if narrow {
        vec![
            Constraint::Percentage(28),
            Constraint::Percentage(26),
            Constraint::Percentage(22),
            Constraint::Percentage(24),
        ]
    } else {
        vec![
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(16),
            Constraint::Length(7),
            Constraint::Length(16),
        ]
    }
}

fn issue_spans(source: &SyncSourcePayload) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut push = |label: &'static str, count: u64, style: Style| {
        if count == 0 {
            return;
        }
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!(
                "{}={}",
                crate::common::i18n::tui::value(label),
                format_number(i64::try_from(count).unwrap_or(i64::MAX))
            ),
            style,
        ));
    };
    push(
        "malformed",
        source.malformed_lines,
        metric_style(theme::warning_fg()),
    );
    push(
        "oversized",
        source.oversized_lines,
        metric_style(theme::warning_fg()),
    );
    push("skipped", source.skipped_lines, theme::muted_style());
    push(
        "accounting",
        source.accounting_anomaly_lines,
        theme::muted_style(),
    );
    if spans.is_empty() {
        vec![Span::styled("-", theme::muted_style())]
    } else {
        spans
    }
}

fn last_run_spans(last_run: &SyncLastRunPayload) -> Vec<Span<'static>> {
    let status_style = match last_run.status.as_str() {
        "success" => metric_style(theme::positive_fg()),
        "cancelled" => theme::muted_style(),
        _ => metric_style(theme::warning_fg()),
    };
    vec![
        Span::styled(tr("last_run"), theme::muted_style()),
        Span::styled(
            crate::common::i18n::tui::value(&last_run.status).to_string(),
            status_style,
        ),
        Span::styled("  ", theme::muted_style()),
        Span::styled(
            last_run.command.clone(),
            metric_style(theme::metric_input()),
        ),
        Span::styled(tr("started"), theme::muted_style()),
        Span::styled(last_run.started_at.clone(), theme::muted_style()),
    ]
}

fn headline_text(key: &str) -> &'static str {
    match key {
        "syncCenter.headline.ready" => tr("ready_to_sync"),
        "syncCenter.headline.busy" => tr("sync_is_running"),
        "syncCenter.headline.failed" => tr("recent_sync_failed"),
        "syncCenter.headline.rebuildRisk" => tr("rebuild_risk_2"),
        _ => tr("sync_status"),
    }
}

fn reason_text(key: &str) -> &'static str {
    match key {
        "syncCenter.reason.ready" => tr("local_status_looks_usable"),
        "syncCenter.reason.empty" => tr("no_sync_status_has_been_recorded"),
        "syncCenter.reason.lastRunFailed" => tr("the_latest_usage_import_failed"),
        "syncCenter.reason.rebuildRisk" => tr("missing_source_files_protect_stored_history"),
        _ => tr("check_sync_details_below"),
    }
}

fn tone_style(tone: &str) -> Style {
    match tone {
        "good" => metric_style(theme::positive_fg()),
        "warn" => metric_style(theme::warning_fg()),
        "neutral" => theme::muted_style(),
        _ => Style::default(),
    }
}

fn metric_style(color: Color) -> Style {
    theme::bold_fg_style(color)
}

fn yes_no(value: bool) -> &'static str {
    if value { tr("yes") } else { tr("no") }
}
