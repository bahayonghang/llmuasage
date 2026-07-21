//! Aligned, optionally colored rendering of the final `Sync finished` block.
//!
//! `format_summary_lines` is a pure function so alignment, coloring, and edge
//! cases (absent sources, errors, zero values) are unit-testable without a
//! terminal. Widths are computed on plain text first; ANSI styles are applied
//! only after alignment so escape sequences never skew column widths.

use console::Style;

use crate::{commands::sync::SyncSummary, models::SourceKind, parsers::SourceSyncStats};

const HEADERS: [&str; 10] = [
    "SOURCE",
    "FILES",
    "CHANGED",
    "SKIPPED",
    "SEEN",
    "COMMITTED",
    "STORED",
    "BYTES",
    "PARSE",
    "WRITE",
];

/// Renders the full summary block: title, optional rebuild hint, the aligned
/// per-source table, and the totals line.
pub(crate) fn format_summary_lines(
    summary: &SyncSummary,
    rebuild: bool,
    color: bool,
) -> Vec<String> {
    let rows: Vec<Row> = summary.sources.iter().map(Row::from_stats).collect();
    let mut widths = [0usize; HEADERS.len()];
    for (index, header) in HEADERS.iter().enumerate() {
        widths[index] = header.len();
    }
    for row in &rows {
        for (index, cell) in row.cells.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    let mut lines = vec![styled(Style::new().bold(), "Sync finished:", color)];
    if rebuild {
        lines
            .push("- rebuild: reset parser-backed usage state source by source before sync".into());
    }

    let header_line = HEADERS
        .iter()
        .enumerate()
        .map(|(index, header)| pad(header, widths[index], Align::Left))
        .collect::<Vec<_>>()
        .join("  ");
    lines.push(styled(Style::new().dim(), &header_line, color));

    for (row, stats) in rows.iter().zip(summary.sources.iter()) {
        lines.push(render_row(row, stats, &widths, color));
        if let Some(error) = &stats.last_error {
            lines.push(styled(Style::new().red(), &format!("  ↳ {error}"), color));
        }
    }

    lines.push(format!(
        "- totals: seen={} inserted_delta={} stored_events={}",
        summary.total_seen, summary.total_inserted, summary.stored_events
    ));
    lines
}

enum Align {
    Left,
    Right,
}

fn pad(text: &str, width: usize, align: Align) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_string();
    }
    let padding = " ".repeat(width - len);
    match align {
        Align::Left => format!("{text}{padding}"),
        Align::Right => format!("{padding}{text}"),
    }
}

fn styled(style: Style, text: &str, color: bool) -> String {
    if color {
        style.force_styling(true).apply_to(text).to_string()
    } else {
        text.to_string()
    }
}

struct Row {
    cells: [String; HEADERS.len()],
}

impl Row {
    fn from_stats(stats: &SourceSyncStats) -> Self {
        let numeric = |value: usize| {
            if stats.absent {
                "-".to_string()
            } else {
                value.to_string()
            }
        };
        Self {
            cells: [
                source_label(stats.source).to_string(),
                numeric(stats.files_processed),
                numeric(stats.changed_files),
                numeric(stats.skipped_files),
                numeric(stats.events_seen),
                numeric(stats.events_inserted),
                numeric(stats.stored_events),
                if stats.absent {
                    "-".into()
                } else {
                    human_bytes(stats.bytes_scanned)
                },
                if stats.absent {
                    "-".into()
                } else {
                    human_ms(stats.parse_ms)
                },
                if stats.absent {
                    "-".into()
                } else {
                    human_ms(stats.write_ms)
                },
            ],
        }
    }
}

fn render_row(row: &Row, stats: &SourceSyncStats, widths: &[usize], color: bool) -> String {
    row.cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let align = if index == 0 {
                Align::Left
            } else {
                Align::Right
            };
            let padded = pad(cell, widths[index], align);
            match index {
                0 => styled(Style::new().bold(), &padded, color),
                // CHANGED
                2 if stats.changed_files > 0 => styled(Style::new().yellow(), &padded, color),
                // COMMITTED
                5 if stats.events_inserted > 0 => styled(Style::new().green(), &padded, color),
                _ => padded,
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn source_label(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Codex => "codex",
        SourceKind::Claude => "claude",
        SourceKind::Opencode => "opencode",
        SourceKind::Antigravity => "antigravity",
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn human_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(source: SourceKind) -> SourceSyncStats {
        SourceSyncStats {
            source,
            files_processed: 1812,
            changed_files: 18,
            skipped_files: 1794,
            bytes_scanned: 7_800_000,
            events_seen: 104_070,
            events_inserted: 2_461,
            stored_events: 104_050,
            parse_ms: 4_100,
            write_ms: 350,
            ..SourceSyncStats::default()
        }
    }

    fn summary() -> SyncSummary {
        let mut claude = stats(SourceKind::Claude);
        claude.files_processed = 698;
        claude.events_inserted = 0;
        claude.changed_files = 0;
        let mut opencode = stats(SourceKind::Opencode);
        opencode.absent = true;
        opencode.last_error = Some("OpenCode SQLite DB 缺失".to_string());
        SyncSummary {
            sources: vec![stats(SourceKind::Codex), claude, opencode],
            total_seen: 179_317,
            total_inserted: 134_015,
            stored_events: 134_015,
        }
    }

    #[test]
    fn colorless_output_contains_no_ansi_and_aligns_columns() {
        let lines = format_summary_lines(&summary(), false, false);
        assert!(lines.iter().all(|line| !line.contains('\u{1b}')));
        let header = lines.iter().find(|line| line.contains("SOURCE")).unwrap();
        let codex = lines.iter().find(|line| line.starts_with("codex")).unwrap();
        // 右对齐数字列：列尾位置在 header 与数据行对齐。
        let column = header.find("COMMITTED").unwrap();
        let end = column + "COMMITTED".len() - 1;
        assert_eq!(header[end..].chars().next(), Some('D'));
        // codex committed=2461，列尾是个位数字 1。
        assert_eq!(&codex[end..end + 1], "1");
    }

    #[test]
    fn table_shows_human_bytes_and_durations() {
        let lines = format_summary_lines(&summary(), false, false);
        let codex = lines.iter().find(|line| line.starts_with("codex")).unwrap();
        assert!(codex.contains("7.4 MB"), "{codex}");
        assert!(codex.contains("4.1s"), "{codex}");
        assert!(codex.contains("350ms"), "{codex}");
    }

    #[test]
    fn absent_source_uses_placeholders_and_error_line() {
        let lines = format_summary_lines(&summary(), false, false);
        let opencode = lines
            .iter()
            .find(|line| line.starts_with("opencode"))
            .unwrap();
        assert!(opencode.contains('-'), "{opencode}");
        assert!(!opencode.contains("104070"), "{opencode}");
        assert!(
            lines
                .iter()
                .any(|line| line.contains("OpenCode SQLite DB 缺失"))
        );
    }

    #[test]
    fn totals_line_keeps_legacy_shape() {
        let lines = format_summary_lines(&summary(), true, false);
        assert!(lines[0] == "Sync finished:");
        assert!(lines.iter().any(|line| line.starts_with("- rebuild:")));
        assert!(lines.iter().any(|line| {
            line.starts_with("- totals: seen=179317 inserted_delta=134015 stored_events=134015")
        }));
    }

    #[test]
    fn colored_output_wraps_but_preserves_alignment_math() {
        let plain = format_summary_lines(&summary(), false, false);
        let colored = format_summary_lines(&summary(), false, true);
        assert!(colored.iter().any(|line| line.contains('\u{1b}')));
        // 着色只包裹内容，不改变去色后的文本。
        let strip = |text: &str| {
            let mut out = String::new();
            let mut chars = text.chars();
            while let Some(ch) = chars.next() {
                if ch == '\u{1b}' {
                    for inner in chars.by_ref() {
                        if inner == 'm' {
                            break;
                        }
                    }
                } else {
                    out.push(ch);
                }
            }
            out
        };
        for (plain_line, colored_line) in plain.iter().zip(colored.iter()) {
            assert_eq!(*plain_line, strip(colored_line));
        }
    }

    #[test]
    fn zero_and_large_values_format_safely() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(917_506_048), "875.0 MB");
        assert_eq!(human_ms(0), "0ms");
        assert_eq!(human_ms(999), "999ms");
        assert_eq!(human_ms(102_100), "102.1s");
    }
}
