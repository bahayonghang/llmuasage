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
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(theme::panel_block("Usage / Sync"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("Usage / Sync"));
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
    let block = theme::panel_block("Usage / Sync");
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
            Span::styled("events ", theme::muted_style()),
            Span::styled(
                format_number(payload.metrics.events_seen),
                metric_style(theme::metric_input()),
            ),
            Span::styled("  inserted ", theme::muted_style()),
            Span::styled(
                format_number(payload.metrics.inserted_delta),
                metric_style(theme::positive_fg()),
            ),
            Span::styled("  stored ", theme::muted_style()),
            Span::styled(
                format_number(payload.metrics.stored_events),
                metric_style(theme::warning_fg()),
            ),
            Span::styled("  sources ", theme::muted_style()),
            Span::styled(
                format!("{}/{}", payload.metrics.sources_ready, source_total),
                metric_style(theme::metric_cache_write()),
            ),
        ]),
        Line::from(vec![
            Span::styled("lock ", theme::muted_style()),
            Span::styled(
                payload.safety.worker_lock.as_str(),
                if payload.safety.worker_lock == "available" {
                    metric_style(theme::positive_fg())
                } else {
                    metric_style(theme::warning_fg())
                },
            ),
            Span::styled("  rebuild-risk ", theme::muted_style()),
            Span::styled(
                yes_no(payload.safety.lossy_rebuild_risk),
                if payload.safety.lossy_rebuild_risk {
                    metric_style(theme::warning_fg())
                } else {
                    metric_style(theme::positive_fg())
                },
            ),
            Span::styled("  monitored ", theme::muted_style()),
            Span::styled(
                format!("{detected}/{} detected", probes.len()),
                metric_style(theme::metric_input()),
            ),
            Span::styled("  parserless ", theme::muted_style()),
            Span::styled(monitor_only.to_string(), metric_style(theme::warning_fg())),
        ]),
    ];

    if let Some(last_run) = &payload.last_run {
        lines.push(Line::from(last_run_spans(last_run)));
    } else {
        lines.push(Line::styled(
            "last run: none recorded",
            theme::muted_style(),
        ));
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
        let empty = Paragraph::new("No source sync status yet. Run sync or refresh after imports.")
            .style(theme::muted_style())
            .block(theme::trend_card_block(
                "Source Sync",
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
            "Source Sync",
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
        "monitor-only detected: none".to_string()
    } else {
        format!("monitor-only detected: {}", detected_names.join(", "))
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("platform probes ", theme::muted_style()),
            Span::styled(
                format!("{detected}/{}", probes.len()),
                metric_style(theme::metric_input()),
            ),
            Span::styled("  blocked-no-samples ", theme::muted_style()),
            Span::styled(
                parserless_blocked.to_string(),
                metric_style(theme::warning_fg()),
            ),
        ]),
        Line::styled(detected_line, theme::muted_style()),
        Line::styled(
            "parserless platforms stay monitor-only until fixtures and token semantics exist",
            theme::muted_style(),
        ),
    ];
    let widget = Paragraph::new(lines).block(theme::trend_card_block(
        "Platform Monitor",
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
            Cell::from(source.status.clone()).style(tone_style(&source.tone)),
            Cell::from(format_number(source.stored_events)),
        ])
    } else if narrow {
        Row::new(vec![
            Cell::from(source.source.clone()),
            Cell::from(source.status.clone()).style(tone_style(&source.tone)),
            Cell::from(format_number(source.events_seen)),
            Cell::from(format_number(source.stored_events)),
        ])
    } else {
        Row::new(vec![
            Cell::from(source.source.clone()).style(theme::bold_style()),
            Cell::from(source.status.clone()).style(tone_style(&source.tone)),
            Cell::from(format_number(source.events_seen)),
            Cell::from(format_number(source.events_inserted)),
            Cell::from(format_number(source.skipped_files)),
            Cell::from(format_number(source.stored_events)),
            Cell::from(format!("{:.0}%", source.share * 100.0)),
            Cell::from(source.updated_at.clone().unwrap_or_else(|| "-".to_string())),
        ])
    };

    if source.lossy_rebuild_risk {
        row = row.style(metric_style(theme::warning_fg()));
    } else if index % 2 == 1 {
        row = row.style(theme::row_alt_style());
    }
    row
}

fn source_header(very_narrow: bool, narrow: bool) -> Vec<Cell<'static>> {
    let labels: &[&str] = if very_narrow {
        &["Source", "Status", "Stored"]
    } else if narrow {
        &["Source", "Status", "Seen", "Stored"]
    } else {
        &[
            "Source", "Status", "Seen", "Inserted", "Skipped", "Stored", "Share", "Updated",
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
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(18),
        ]
    }
}

fn last_run_spans(last_run: &SyncLastRunPayload) -> Vec<Span<'static>> {
    let status_style = if last_run.status == "success" {
        metric_style(theme::positive_fg())
    } else {
        metric_style(theme::warning_fg())
    };
    vec![
        Span::styled("last run ", theme::muted_style()),
        Span::styled(last_run.status.clone(), status_style),
        Span::styled("  ", theme::muted_style()),
        Span::styled(
            last_run.command.clone(),
            metric_style(theme::metric_input()),
        ),
        Span::styled("  started ", theme::muted_style()),
        Span::styled(last_run.started_at.clone(), theme::muted_style()),
    ]
}

fn headline_text(key: &str) -> &'static str {
    match key {
        "syncCenter.headline.ready" => "Ready to sync",
        "syncCenter.headline.busy" => "Sync is running",
        "syncCenter.headline.failed" => "Recent sync failed",
        "syncCenter.headline.rebuildRisk" => "Rebuild risk",
        _ => "Sync status",
    }
}

fn reason_text(key: &str) -> &'static str {
    match key {
        "syncCenter.reason.ready" => "local status looks usable",
        "syncCenter.reason.empty" => "no sync status has been recorded",
        "syncCenter.reason.rebuildRisk" => "missing source files protect stored history",
        _ => "check sync details below",
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
    if value { "yes" } else { "no" }
}
