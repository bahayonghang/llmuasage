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
const COMPACT_HEADERS: [&str; HEADERS.len()] = [
    "SRC", "F", "CHG", "SKIP", "SEEN", "COMMIT", "STORE", "BYTES", "PARSE", "WRITE",
];

/// Renders the full summary block: title, optional rebuild hint, the aligned
/// per-source table, and the aggregated `TOTAL` row.
///
/// `terminal_width` keeps the function pure and unit-testable: on a narrow
/// terminal only the SOURCE label column is shrunk (numeric columns are never
/// truncated), mirroring the convention in `src/tui/report_table.rs`.
pub(crate) fn format_summary_lines(
    summary: &SyncSummary,
    rebuild: bool,
    color: bool,
    terminal_width: usize,
) -> Vec<String> {
    let rows: Vec<Row> = summary.sources.iter().map(Row::from_stats).collect();
    let total = total_cells(summary);
    let full_widths = column_widths(&HEADERS, &rows, &total);
    let compact = table_width(&full_widths, 2) > terminal_width;
    let headers = if compact { &COMPACT_HEADERS } else { &HEADERS };
    let gap = if compact { 1 } else { 2 };
    let separator = " ".repeat(gap);
    let mut widths = column_widths(headers, &rows, &total);
    fit_source_column(&mut widths, headers[0].chars().count(), terminal_width, gap);

    let mut lines = vec![styled(Style::new().bold(), "Sync finished:", color)];
    if rebuild {
        lines
            .push("- rebuild: reset parser-backed usage state source by source before sync".into());
    }

    let header_line = headers
        .iter()
        .enumerate()
        .map(|(index, header)| pad(header, widths[index], Align::Left))
        .collect::<Vec<_>>()
        .join(&separator);
    lines.push(styled(Style::new().dim(), &header_line, color));

    for (row, stats) in rows.iter().zip(summary.sources.iter()) {
        lines.push(render_row(row, stats, &widths, &separator, color));
        if let Some(error) = &stats.last_error {
            lines.push(styled(Style::new().red(), &format!("  ↳ {error}"), color));
        }
    }

    lines.push(render_total_row(&total, &widths, &separator, color));
    lines
}

fn column_widths(
    headers: &[&str; HEADERS.len()],
    rows: &[Row],
    total: &[String; HEADERS.len()],
) -> [usize; HEADERS.len()] {
    let mut widths = std::array::from_fn(|index| headers[index].chars().count());
    for cells in rows
        .iter()
        .map(|row| &row.cells)
        .chain(std::iter::once(total))
    {
        for (index, cell) in cells.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    widths
}

fn table_width(widths: &[usize; HEADERS.len()], gap: usize) -> usize {
    widths.iter().sum::<usize>() + gap * (HEADERS.len() - 1)
}

/// Aggregates every `TOTAL` cell from the per-source stats. This keeps the
/// terminal projection independent from the summary/event aggregate fields.
fn total_cells(summary: &SyncSummary) -> [String; HEADERS.len()] {
    let sum_usize = |value: fn(&SourceSyncStats) -> usize| {
        summary
            .sources
            .iter()
            .fold(0usize, |total, stats| total.saturating_add(value(stats)))
    };
    let sum_u64 = |value: fn(&SourceSyncStats) -> u64| {
        summary
            .sources
            .iter()
            .fold(0u64, |total, stats| total.saturating_add(value(stats)))
    };
    [
        "TOTAL".to_string(),
        sum_usize(|stats| stats.files_processed).to_string(),
        sum_usize(|stats| stats.changed_files).to_string(),
        sum_usize(|stats| stats.skipped_files).to_string(),
        sum_usize(|stats| stats.events_seen).to_string(),
        sum_usize(|stats| stats.events_inserted).to_string(),
        sum_usize(|stats| stats.stored_events).to_string(),
        human_bytes(sum_u64(|stats| stats.bytes_scanned)),
        human_ms(sum_u64(|stats| stats.parse_ms)),
        human_ms(sum_u64(|stats| stats.write_ms)),
    ]
}

/// Shrinks only the SOURCE column when the natural table is wider than the
/// terminal. Numeric columns are never narrowed; if they alone still overflow
/// the numbers stay intact (accept overflow rather than corrupt values).
fn fit_source_column(
    widths: &mut [usize; HEADERS.len()],
    min_source: usize,
    terminal_width: usize,
    gap: usize,
) {
    let natural = table_width(widths, gap);
    if natural <= terminal_width {
        return;
    }
    let overflow = natural - terminal_width;
    widths[0] = widths[0].saturating_sub(overflow).max(min_source);
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

fn render_row(
    row: &Row,
    stats: &SourceSyncStats,
    widths: &[usize],
    separator: &str,
    color: bool,
) -> String {
    row.cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let align = if index == 0 {
                Align::Left
            } else {
                Align::Right
            };
            // Only the SOURCE label may be truncated; numeric cells stay intact.
            let text = if index == 0 {
                fit_label(cell, widths[0])
            } else {
                cell.clone()
            };
            let padded = pad(&text, widths[index], align);
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
        .join(separator)
}

/// Renders the aggregated `TOTAL` row through the same width/pad/align
/// machinery as source rows: bold label, right-aligned numeric cells, no
/// semantic accent colors and no separator glyph.
fn render_total_row(
    cells: &[String; HEADERS.len()],
    widths: &[usize],
    separator: &str,
    color: bool,
) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let align = if index == 0 {
                Align::Left
            } else {
                Align::Right
            };
            let text = if index == 0 {
                fit_label(cell, widths[0])
            } else {
                cell.clone()
            };
            let padded = pad(&text, widths[index], align);
            if index == 0 {
                styled(Style::new().bold(), &padded, color)
            } else {
                padded
            }
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// Truncates an over-long display label to `width`, ending with `…`. Applied
/// only to the SOURCE column so numeric cells are never shortened.
fn fit_label(label: &str, width: usize) -> String {
    if label.chars().count() <= width {
        return label.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut truncated: String = label.chars().take(width - 1).collect();
    truncated.push('…');
    truncated
}

/// Table label for a source, read from the static descriptor so a new
/// `SourceKind` variant populates this display surface with no edit here.
fn source_label(source: SourceKind) -> &'static str {
    crate::registry::source_descriptor(source)
        .map_or_else(|| source.as_str(), |descriptor| descriptor.stable_id)
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

    /// Wide terminal used by alignment assertions so natural widths are kept.
    const WIDE: usize = 200;

    /// Strips ANSI SGR sequences so colored output can be compared to plain.
    fn strip_ansi(text: &str) -> String {
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
    }

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
        let opencode = SourceSyncStats {
            source: SourceKind::Opencode,
            absent: true,
            last_error: Some("OpenCode SQLite DB 缺失".to_string()),
            ..SourceSyncStats::default()
        };
        SyncSummary {
            sources: vec![stats(SourceKind::Codex), claude, opencode],
            total_seen: 208_140,
            total_inserted: 2_461,
            stored_events: 208_100,
        }
    }

    #[test]
    fn colorless_output_contains_no_ansi_and_aligns_columns() {
        let lines = format_summary_lines(&summary(), false, false, WIDE);
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
        let lines = format_summary_lines(&summary(), false, false, WIDE);
        let codex = lines.iter().find(|line| line.starts_with("codex")).unwrap();
        assert!(codex.contains("7.4 MB"), "{codex}");
        assert!(codex.contains("4.1s"), "{codex}");
        assert!(codex.contains("350ms"), "{codex}");
    }

    #[test]
    fn absent_source_uses_placeholders_and_error_line() {
        let lines = format_summary_lines(&summary(), false, false, WIDE);
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
    fn total_row_replaces_legacy_totals_line() {
        let lines = format_summary_lines(&summary(), true, false, WIDE);
        assert_eq!(lines[0], "Sync finished:");
        assert!(lines.iter().any(|line| line.starts_with("- rebuild:")));
        // The standalone `- totals:` line is gone; a TOTAL table row replaces it.
        assert!(!lines.iter().any(|line| line.starts_with("- totals:")));
        let total = lines
            .iter()
            .find(|line| line.starts_with("TOTAL"))
            .expect("TOTAL row present");
        assert!(total.contains("208140"), "{total}");
        assert!(total.contains("208100"), "{total}");
        // FILES sums non-absent sources: codex 1812 + claude 698 (opencode absent).
        assert!(total.contains("2510"), "{total}");
        // SKIPPED sums non-absent sources: 1794 + 1794.
        assert!(total.contains("3588"), "{total}");
    }

    #[test]
    fn total_row_aggregates_source_stats() {
        let lines = format_summary_lines(&summary(), false, false, WIDE);
        let total = lines
            .iter()
            .find(|line| line.starts_with("TOTAL"))
            .expect("TOTAL row present");
        // BYTES / PARSE / WRITE aggregate the source rows.
        assert!(total.contains("14.9 MB"), "{total}"); // 7.8M + 7.8M bytes
        assert!(total.contains("8.2s"), "{total}"); // 4100ms + 4100ms
        assert!(total.contains("700ms"), "{total}"); // 350ms + 350ms
        // The TOTAL row carries no ANSI in colorless mode.
        assert!(!total.contains('\u{1b}'), "{total}");

        let mut inconsistent = summary();
        inconsistent.total_seen = usize::MAX;
        inconsistent.total_inserted = usize::MAX;
        inconsistent.stored_events = usize::MAX;
        let cells = total_cells(&inconsistent);
        assert_eq!(cells[4], "208140");
        assert_eq!(cells[5], "2461");
        assert_eq!(cells[6], "208100");
    }

    #[test]
    fn narrow_width_truncates_only_source_label() {
        // A single long-labelled source ("antigravity") forces the SOURCE column
        // to shrink on a narrow terminal.
        let summary = SyncSummary {
            sources: vec![stats(SourceKind::Antigravity)],
            total_seen: 179_317,
            total_inserted: 134_015,
            stored_events: 134_015,
        };
        let lines = format_summary_lines(&summary, false, false, 60);
        let row = lines
            .iter()
            .find(|line| line.contains('…'))
            .expect("long source label truncates with an ellipsis");
        assert!(row.starts_with("antig…"), "{row}");
        // Numeric cells are never truncated even when the label is.
        assert!(row.contains("104070"), "{row}"); // events_seen
        assert!(row.contains("2461"), "{row}"); // events_inserted
        let total = lines
            .iter()
            .find(|line| strip_ansi(line).starts_with("TOTAL"))
            .expect("TOTAL row present");
        assert!(total.contains("104070"), "{total}");
        assert!(total.contains("104050"), "{total}");
        let table_lines = lines
            .iter()
            .filter(|line| line.contains("SRC") || line.contains('…') || line.starts_with("TOTAL"));
        assert!(
            table_lines.clone().all(|line| line.chars().count() <= 60),
            "{}",
            table_lines.cloned().collect::<Vec<_>>().join("\n")
        );
        let header = lines.iter().find(|line| line.contains("SRC")).unwrap();
        assert!(header.contains("CHG"), "{header}");
        assert!(!header.contains("CHANGED"), "{header}");
    }

    #[test]
    fn empty_summary_still_renders_zero_total_row() {
        let summary = SyncSummary {
            sources: Vec::new(),
            total_seen: 99,
            total_inserted: 99,
            stored_events: 99,
        };
        let lines = format_summary_lines(&summary, false, false, 60);
        let total = lines
            .iter()
            .find(|line| line.starts_with("TOTAL"))
            .expect("TOTAL row present");
        assert_eq!(
            total.split_whitespace().collect::<Vec<_>>(),
            vec![
                "TOTAL", "0", "0", "0", "0", "0", "0", "0", "B", "0ms", "0ms"
            ]
        );
    }

    #[test]
    fn colored_output_wraps_but_preserves_alignment_math() {
        let plain = format_summary_lines(&summary(), false, false, WIDE);
        let colored = format_summary_lines(&summary(), false, true, WIDE);
        assert!(colored.iter().any(|line| line.contains('\u{1b}')));
        // 着色只包裹内容，不改变去色后的文本。
        for (plain_line, colored_line) in plain.iter().zip(colored.iter()) {
            assert_eq!(*plain_line, strip_ansi(colored_line));
        }
    }

    #[test]
    fn colored_total_row_matches_plain_after_strip() {
        let plain = format_summary_lines(&summary(), false, false, WIDE);
        let colored = format_summary_lines(&summary(), false, true, WIDE);
        let plain_total = plain
            .iter()
            .find(|line| line.starts_with("TOTAL"))
            .expect("plain TOTAL row");
        let colored_total = colored
            .iter()
            .find(|line| strip_ansi(line).starts_with("TOTAL"))
            .expect("colored TOTAL row");
        assert!(colored_total.contains('\u{1b}'), "{colored_total}");
        assert_eq!(*plain_total, strip_ansi(colored_total));
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
