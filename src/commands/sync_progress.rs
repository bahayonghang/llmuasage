//! Human-mode progress rendering for `llmusage sync`.
//!
//! One event entry point and one copy source (`human_progress_line`) feed two
//! renderers: `LineRenderer` keeps the legacy line-by-line stderr behavior for
//! non-TTY output and `LLMUSAGE_PROGRESS=off`, while `BarRenderer` draws
//! indicatif bars on a TTY. `TerminalGuard` gives the command layer RAII
//! cleanup so every exit path (early `?`, failure, cancel, success) leaves no
//! dangling bar behind.

use std::{
    ffi::OsString,
    io::{IsTerminal, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::{models::SourceKind, parsers::SyncEvent};

const SPINNER_TICK: Duration = Duration::from_millis(100);
const DRAW_HZ: u8 = 10;
const MAX_FILE_LABEL_CHARS: usize = 40;

/// Production renderer for the CLI: indicatif bars when stderr is a TTY and
/// `LLMUSAGE_PROGRESS` is unset/empty, plain lines otherwise.
pub(crate) fn stderr_renderer() -> HumanRenderer {
    let draw = if std::io::stderr().is_terminal() {
        ProgressDrawTarget::stderr_with_hz(DRAW_HZ)
    } else {
        ProgressDrawTarget::hidden()
    };
    HumanRenderer::new(draw, progress_env_forces_line())
}

fn progress_env_forces_line() -> bool {
    forced_line(std::env::var_os("LLMUSAGE_PROGRESS"))
}

/// Any non-empty `LLMUSAGE_PROGRESS` value forces the line renderer
/// (profiling A/B switch and user fallback).
fn forced_line(value: Option<OsString>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// RAII cleanup owned by the command function itself (not the reporter task),
/// so bootstrap-phase `?` early returns also finish the renderer.
pub(crate) struct TerminalGuard {
    renderer: Arc<Mutex<HumanRenderer>>,
}

impl TerminalGuard {
    pub(crate) fn new(renderer: Arc<Mutex<HumanRenderer>>) -> Self {
        Self { renderer }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Ok(mut renderer) = self.renderer.lock() {
            renderer.finish();
        }
    }
}

pub(crate) enum HumanRenderer {
    Line(LineRenderer),
    Bar(BarRenderer),
}

impl HumanRenderer {
    /// Injectable draw target: hidden (non-TTY) or `force_line` selects the
    /// line renderer; anything visible selects the bar renderer.
    pub(crate) fn new(draw: ProgressDrawTarget, force_line: bool) -> Self {
        if force_line || draw.is_hidden() {
            Self::Line(LineRenderer::new())
        } else {
            Self::Bar(BarRenderer::new(draw))
        }
    }

    pub(crate) fn render(&mut self, event: &SyncEvent) {
        match self {
            Self::Line(renderer) => renderer.render(event),
            Self::Bar(renderer) => renderer.render(event),
        }
    }

    /// Idempotent teardown: abandon any active bar, stop ticks, restore the
    /// terminal. Safe to call from [`TerminalGuard`] after explicit finishes.
    pub(crate) fn finish(&mut self) {
        match self {
            Self::Line(renderer) => renderer.finish(),
            Self::Bar(renderer) => renderer.finish(),
        }
    }

    #[cfg(test)]
    fn has_active_bar(&self) -> bool {
        match self {
            Self::Line(_) => false,
            Self::Bar(renderer) => renderer.has_active(),
        }
    }
}

/// Accumulated render cost, reported at the end of a run so profiling can
/// prove progress rendering stays off the parser hot path.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RenderStats {
    pub(crate) calls: u64,
    pub(crate) nanos: u128,
}

/// Shared render entry used by the bootstrap sink and the reporter task.
pub(crate) fn render_shared(renderer: &Arc<Mutex<HumanRenderer>>, event: &SyncEvent) {
    if let Ok(mut renderer) = renderer.lock() {
        renderer.render(event);
    }
}

/// Records one render call's wall time against the shared counter.
pub(crate) fn render_shared_timed(
    renderer: &Arc<Mutex<HumanRenderer>>,
    stats: &Arc<Mutex<RenderStats>>,
    event: &SyncEvent,
) {
    let started = std::time::Instant::now();
    render_shared(renderer, event);
    if let Ok(mut stats) = stats.lock() {
        stats.calls += 1;
        stats.nanos += started.elapsed().as_nanos();
    }
}

/// Legacy line-by-line stderr renderer (former `HumanProgress`), preserved
/// verbatim for non-TTY output and the `LLMUSAGE_PROGRESS=off` fallback.
pub(crate) struct LineRenderer {
    stderr: std::io::Stderr,
    tty: bool,
    last_line_len: usize,
    terminated: bool,
}

impl LineRenderer {
    pub(crate) fn new() -> Self {
        let stderr = std::io::stderr();
        let tty = stderr.is_terminal();
        Self {
            stderr,
            tty,
            last_line_len: 0,
            terminated: false,
        }
    }

    fn render(&mut self, event: &SyncEvent) {
        if self.terminated {
            return;
        }
        if matches!(event, SyncEvent::Failed { .. } | SyncEvent::Cancelled) {
            self.terminated = true;
        }
        let Some(line) = human_progress_line(event) else {
            return;
        };
        if self.tty {
            let padding = self.last_line_len.saturating_sub(line.chars().count());
            let _ = write!(self.stderr, "\r{line}{}", " ".repeat(padding));
            let _ = self.stderr.flush();
            self.last_line_len = line.chars().count();
            if matches!(
                event,
                SyncEvent::SourceFinished { .. }
                    | SyncEvent::MigrationFinished { .. }
                    | SyncEvent::PricingBucketReconcileStarted { .. }
                    | SyncEvent::PricingUpgradeFinished { .. }
                    | SyncEvent::LockAcquired { .. }
            ) {
                let _ = writeln!(self.stderr);
                self.last_line_len = 0;
            }
        } else {
            let _ = writeln!(self.stderr, "{line}");
        }
    }

    fn finish(&mut self) {
        let _ = self.stderr.flush();
    }
}

/// OpenCode reports row counts against `files_total = 1`, so a determinate
/// bar would overflow; only Codex/Claude replay-file progress is determinate.
fn uses_determinate_bar(source: SourceKind) -> bool {
    matches!(source, SourceKind::Codex | SourceKind::Claude)
}

struct ActiveBar {
    bar: ProgressBar,
    determinate: bool,
}

/// TTY renderer backed by an indicatif [`MultiProgress`]. One bar is active
/// at a time; phase boundaries land as permanent lines above the bars.
pub(crate) struct BarRenderer {
    multi: MultiProgress,
    active: Option<ActiveBar>,
    terminated: bool,
}

impl BarRenderer {
    pub(crate) fn new(draw: ProgressDrawTarget) -> Self {
        Self {
            multi: MultiProgress::with_draw_target(draw),
            active: None,
            terminated: false,
        }
    }

    fn render(&mut self, event: &SyncEvent) {
        if self.terminated {
            return;
        }
        match event {
            SyncEvent::BootstrapStarted
            | SyncEvent::MigrationStarted { .. }
            | SyncEvent::LockWaiting { .. } => {
                self.start_spinner(human_progress_line(event).unwrap_or_default());
            }
            SyncEvent::PricingUpgradeStarted { total_events, .. } => {
                let line = human_progress_line(event).unwrap_or_default();
                if *total_events > 0 {
                    self.start_bar(*total_events as u64, line);
                } else {
                    self.start_spinner(line);
                }
            }
            SyncEvent::PricingUpgradeProgress {
                processed_events, ..
            } => {
                if let Some(active) = &self.active
                    && active.determinate
                {
                    let len = active.bar.length().unwrap_or(0);
                    active.bar.set_position((*processed_events as u64).min(len));
                    if let Some(line) = human_progress_line(event) {
                        active.bar.set_message(line);
                    }
                }
            }
            SyncEvent::PricingBucketReconcileStarted { .. } => {
                // Boundary: close the repricing bar with a permanent line,
                // then spin while buckets reconcile.
                self.permanent_line(event);
                self.start_spinner(human_progress_line(event).unwrap_or_default());
            }
            SyncEvent::MigrationFinished { .. }
            | SyncEvent::PricingUpgradeFinished { .. }
            | SyncEvent::LockAcquired { .. }
            | SyncEvent::SourceFinished { .. } => {
                self.permanent_line(event);
            }
            SyncEvent::SourceStarted {
                source,
                files_total,
            } => {
                let line = human_progress_line(event).unwrap_or_default();
                if uses_determinate_bar(*source) {
                    self.start_bar(*files_total, line);
                } else {
                    self.start_spinner(line);
                }
            }
            SyncEvent::Progress {
                source,
                files_scanned,
                records_imported,
                current_file,
            } => {
                self.render_source_progress(
                    *source,
                    *files_scanned,
                    *records_imported,
                    current_file.as_deref(),
                );
            }
            SyncEvent::Failed { .. } | SyncEvent::Cancelled => {
                self.terminated = true;
                let line = human_progress_line(event).unwrap_or_default();
                if let Some(active) = self.active.take() {
                    active.bar.abandon_with_message(line.clone());
                }
                self.permanent_text(line);
            }
            SyncEvent::Started { .. }
            | SyncEvent::Finished { .. }
            | SyncEvent::RecentReady { .. } => {}
        }
    }

    fn render_source_progress(
        &mut self,
        source: SourceKind,
        files_scanned: u64,
        records_imported: u64,
        current_file: Option<&str>,
    ) {
        let Some(active) = &self.active else {
            return;
        };
        if uses_determinate_bar(source) && active.determinate {
            // Position counts replayed files only; a mid-bar position is a
            // truthful reflection of remaining replay work.
            let len = active.bar.length().unwrap_or(0);
            let pos = files_scanned.min(len);
            active.bar.set_position(pos);
            active.bar.set_message(format!(
                "{}: 重放 {pos}/{len} · 导入 {records_imported} 条",
                source_label(source)
            ));
        } else {
            // OpenCode: the spinner position carries scanned row counts.
            active.bar.set_position(files_scanned);
            let file = current_file.map(short_file_label).unwrap_or_default();
            let message = if file.is_empty() {
                format!("导入 {records_imported} 条")
            } else {
                format!("导入 {records_imported} 条 · {file}")
            };
            active.bar.set_message(message);
        }
    }

    fn start_spinner(&mut self, message: String) {
        self.clear_active();
        let bar = self.multi.add(ProgressBar::new_spinner());
        bar.set_style(spinner_style());
        bar.set_message(message);
        bar.enable_steady_tick(SPINNER_TICK);
        self.active = Some(ActiveBar {
            bar,
            determinate: false,
        });
    }

    fn start_bar(&mut self, len: u64, message: String) {
        self.clear_active();
        let bar = self.multi.add(ProgressBar::new(len));
        bar.set_style(bar_style());
        bar.set_message(message);
        self.active = Some(ActiveBar {
            bar,
            determinate: true,
        });
    }

    /// Phase transition without a boundary event: drop the previous bar.
    fn clear_active(&mut self) {
        if let Some(active) = self.active.take() {
            active.bar.finish_and_clear();
        }
    }

    /// Boundary event: walk determinate bars to full, keep the final bar
    /// state, then land the permanent copy line above the bars.
    fn complete_active(&mut self) {
        if let Some(active) = self.active.take() {
            if active.determinate {
                let len = active.bar.length().unwrap_or(0);
                active.bar.set_position(len);
                active.bar.finish();
            } else {
                active.bar.finish_and_clear();
            }
        }
    }

    fn permanent_line(&mut self, event: &SyncEvent) {
        self.complete_active();
        if let Some(line) = human_progress_line(event) {
            self.permanent_text(line);
        }
    }

    fn permanent_text(&self, line: String) {
        let _ = self.multi.println(line);
    }

    fn finish(&mut self) {
        if let Some(active) = self.active.take() {
            active.bar.abandon();
        }
        let _ = self.multi.clear();
    }

    #[cfg(test)]
    fn has_active(&self) -> bool {
        self.active.is_some()
    }

    #[cfg(test)]
    fn active_is_determinate(&self) -> Option<bool> {
        self.active.as_ref().map(|active| active.determinate)
    }
}

fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner} {pos} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
}

fn bar_style() -> ProgressStyle {
    ProgressStyle::with_template("{msg} [{bar:32}] {pos}/{len}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
}

fn short_file_label(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if name.chars().count() > MAX_FILE_LABEL_CHARS {
        let mut shortened: String = name.chars().take(MAX_FILE_LABEL_CHARS - 1).collect();
        shortened.push('…');
        shortened
    } else {
        name.to_string()
    }
}

pub(crate) fn human_progress_line(event: &SyncEvent) -> Option<String> {
    match event {
        SyncEvent::BootstrapStarted => Some("检查数据库 schema 与定价目录...".to_string()),
        SyncEvent::MigrationStarted {
            version,
            name,
            latest_version,
        } => Some(format!(
            "升级数据库 schema v0 → v{latest_version}，正在执行 v{version} {name}..."
        )),
        SyncEvent::MigrationFinished {
            version,
            name,
            elapsed_ms,
        } => Some(format!(
            "数据库 schema v{version} {name} 完成（{elapsed_ms}ms）"
        )),
        SyncEvent::PricingUpgradeStarted {
            from_version,
            to_version,
            total_events,
        } => Some(format!(
            "升级定价目录 {from_version} → {to_version}：共 {total_events} 条事件..."
        )),
        SyncEvent::PricingUpgradeProgress {
            from_version,
            to_version,
            processed_events,
            total_events,
            elapsed_ms,
        } => {
            let percentage = if *total_events == 0 {
                100.0
            } else {
                *processed_events as f64 * 100.0 / *total_events as f64
            };
            Some(format!(
                "升级定价目录 {from_version} → {to_version}：已处理 {processed_events}/{total_events} 条（{percentage:.1}%），已用 {elapsed_ms}ms"
            ))
        }
        SyncEvent::PricingBucketReconcileStarted {
            to_version,
            bucket_count,
        } => Some(format!(
            "事件定价完成，正在对账 {bucket_count} 个 {to_version} 汇总桶..."
        )),
        SyncEvent::PricingUpgradeFinished {
            to_version,
            updated_events,
            bucket_count,
            deleted_orphan_buckets,
            elapsed_ms,
            ..
        } => Some(format!(
            "定价目录 {to_version} 升级完成：{updated_events} 条事件，{bucket_count} 个桶，清理 {deleted_orphan_buckets} 个孤立桶（{elapsed_ms}ms）"
        )),
        SyncEvent::LockWaiting { .. } => Some("等待 SQLite sync worker 锁...".to_string()),
        SyncEvent::LockAcquired { wait_ms } => {
            Some(format!("已获取 SQLite sync worker 锁（等待 {wait_ms}ms）"))
        }
        SyncEvent::SourceStarted {
            source,
            files_total,
        } => Some(format!(
            "{}: 扫描 {files_total} 个文件...",
            source_label(*source)
        )),
        SyncEvent::Progress {
            source,
            files_scanned,
            records_imported,
            ..
        } => Some(format!(
            "{}: 已处理 {files_scanned}，导入 {records_imported} 条",
            source_label(*source)
        )),
        SyncEvent::SourceFinished { source, stats } => Some(format!(
            "{}: 完成，文件 {} 个，跳过 {} 个，提交 {} 条",
            source_label(*source),
            stats.files_processed,
            stats.skipped_files,
            stats.events_inserted
        )),
        SyncEvent::Failed { error } => Some(format!("同步失败：{error}")),
        SyncEvent::Cancelled => Some("同步已取消".to_string()),
        SyncEvent::Started { .. } | SyncEvent::Finished { .. } | SyncEvent::RecentReady { .. } => {
            None
        }
    }
}

fn source_label(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Codex => "Codex",
        SourceKind::Claude => "Claude",
        SourceKind::Opencode => "OpenCode",
        SourceKind::Antigravity => "Antigravity",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::SourceSyncStats;

    fn hidden_bar_renderer() -> BarRenderer {
        BarRenderer::new(ProgressDrawTarget::hidden())
    }

    fn source_finished(source: SourceKind) -> SyncEvent {
        SyncEvent::SourceFinished {
            source,
            stats: SourceSyncStats {
                source,
                files_processed: 10,
                skipped_files: 8,
                events_inserted: 42,
                ..SourceSyncStats::default()
            },
        }
    }

    #[test]
    fn bar_renderer_success_sequence_converges() {
        let mut renderer = hidden_bar_renderer();
        renderer.render(&SyncEvent::BootstrapStarted);
        assert!(renderer.has_active());
        renderer.render(&SyncEvent::MigrationStarted {
            version: 15,
            name: "add_part_cursor".to_string(),
            latest_version: 15,
        });
        renderer.render(&SyncEvent::MigrationFinished {
            version: 15,
            name: "add_part_cursor".to_string(),
            elapsed_ms: 12,
        });
        assert!(!renderer.has_active());
        renderer.render(&SyncEvent::LockWaiting { timeout_ms: 30_000 });
        assert!(renderer.has_active());
        renderer.render(&SyncEvent::LockAcquired { wait_ms: 3 });
        assert!(!renderer.has_active());

        renderer.render(&SyncEvent::SourceStarted {
            source: SourceKind::Codex,
            files_total: 10,
        });
        assert_eq!(renderer.active_is_determinate(), Some(true));
        renderer.render(&SyncEvent::Progress {
            source: SourceKind::Codex,
            files_scanned: 2,
            records_imported: 40,
            current_file: None,
        });
        renderer.render(&source_finished(SourceKind::Codex));
        assert!(!renderer.has_active(), "source finish must close the bar");

        renderer.finish();
        renderer.finish();
        assert!(!renderer.has_active(), "finish must be idempotent");
    }

    #[test]
    fn bar_renderer_pricing_upgrade_sequence_converges() {
        let mut renderer = hidden_bar_renderer();
        renderer.render(&SyncEvent::PricingUpgradeStarted {
            from_version: "static-v1".to_string(),
            to_version: "static-v2".to_string(),
            total_events: 100,
        });
        assert_eq!(renderer.active_is_determinate(), Some(true));
        renderer.render(&SyncEvent::PricingUpgradeProgress {
            from_version: "static-v1".to_string(),
            to_version: "static-v2".to_string(),
            processed_events: 50,
            total_events: 100,
            elapsed_ms: 10,
        });
        assert!(renderer.has_active());
        renderer.render(&SyncEvent::PricingBucketReconcileStarted {
            to_version: "static-v2".to_string(),
            bucket_count: 7,
        });
        assert_eq!(
            renderer.active_is_determinate(),
            Some(false),
            "reconcile phase spins"
        );
        renderer.render(&SyncEvent::PricingUpgradeFinished {
            from_version: "static-v1".to_string(),
            to_version: "static-v2".to_string(),
            updated_events: 100,
            bucket_count: 7,
            deleted_orphan_buckets: 0,
            elapsed_ms: 20,
        });
        assert!(!renderer.has_active());
        renderer.finish();
    }

    #[test]
    fn bar_renderer_opencode_progress_never_overflows() {
        let mut renderer = hidden_bar_renderer();
        renderer.render(&SyncEvent::SourceStarted {
            source: SourceKind::Opencode,
            files_total: 1,
        });
        assert_eq!(
            renderer.active_is_determinate(),
            Some(false),
            "OpenCode row counts must drive a spinner"
        );
        renderer.render(&SyncEvent::Progress {
            source: SourceKind::Opencode,
            files_scanned: 50_000,
            records_imported: 12_345,
            current_file: Some("C:/Users/x/.local/share/opencode/opencode.db".to_string()),
        });
        assert!(renderer.has_active());
        renderer.render(&source_finished(SourceKind::Opencode));
        assert!(!renderer.has_active());
        renderer.finish();
    }

    #[test]
    fn bar_renderer_failed_abandons_and_ignores_followups() {
        let mut renderer = hidden_bar_renderer();
        renderer.render(&SyncEvent::SourceStarted {
            source: SourceKind::Claude,
            files_total: 5,
        });
        renderer.render(&SyncEvent::Failed {
            error: "boom".to_string(),
        });
        assert!(!renderer.has_active());
        renderer.render(&SyncEvent::Failed {
            error: "again".to_string(),
        });
        renderer.render(&SyncEvent::SourceStarted {
            source: SourceKind::Claude,
            files_total: 5,
        });
        assert!(
            !renderer.has_active(),
            "events after a terminal failure are ignored"
        );
        renderer.finish();
        renderer.finish();
    }

    #[test]
    fn bar_renderer_cancelled_abandons_active_bar() {
        let mut renderer = hidden_bar_renderer();
        renderer.render(&SyncEvent::SourceStarted {
            source: SourceKind::Codex,
            files_total: 5,
        });
        assert!(renderer.has_active());
        renderer.render(&SyncEvent::Cancelled);
        assert!(!renderer.has_active());
        renderer.finish();
        renderer.finish();
    }

    #[test]
    fn terminal_guard_finishes_renderer_on_drop() {
        // Simulates a bootstrap-phase early `?` return: the guard is the only
        // cleanup path because the reporter task was never spawned.
        let renderer = Arc::new(Mutex::new(HumanRenderer::Bar(hidden_bar_renderer())));
        let guard = TerminalGuard::new(Arc::clone(&renderer));
        render_shared(&renderer, &SyncEvent::BootstrapStarted);
        render_shared(
            &renderer,
            &SyncEvent::SourceStarted {
                source: SourceKind::Codex,
                files_total: 3,
            },
        );
        drop(guard);
        let renderer = renderer.lock().expect("renderer lock");
        assert!(
            !renderer.has_active_bar(),
            "guard drop must abandon the active bar"
        );
    }

    #[test]
    fn selection_falls_back_to_line_renderer() {
        assert!(matches!(
            HumanRenderer::new(ProgressDrawTarget::hidden(), false),
            HumanRenderer::Line(_)
        ));
        assert!(matches!(
            HumanRenderer::new(ProgressDrawTarget::stderr_with_hz(10), true),
            HumanRenderer::Line(_)
        ));
    }

    #[test]
    fn forced_line_requires_non_empty_env_value() {
        assert!(!forced_line(None));
        assert!(!forced_line(Some(OsString::new())));
        assert!(forced_line(Some(OsString::from("off"))));
        assert!(forced_line(Some(OsString::from("1"))));
    }

    #[test]
    fn short_file_label_uses_last_segment_and_truncates() {
        assert_eq!(
            short_file_label("C:/data/opencode/opencode.db"),
            "opencode.db"
        );
        assert_eq!(short_file_label("state.db"), "state.db");
        let long = format!("{}\u{4e2d}.db", "a".repeat(80));
        let label = short_file_label(&long);
        assert_eq!(label.chars().count(), MAX_FILE_LABEL_CHARS);
        assert!(label.ends_with('…'));
    }

    #[test]
    fn human_lines_never_contain_ansi_escapes() {
        let events = vec![
            SyncEvent::BootstrapStarted,
            SyncEvent::MigrationStarted {
                version: 15,
                name: "add_part_cursor".to_string(),
                latest_version: 15,
            },
            SyncEvent::MigrationFinished {
                version: 15,
                name: "add_part_cursor".to_string(),
                elapsed_ms: 12,
            },
            SyncEvent::PricingUpgradeStarted {
                from_version: "static-v1".to_string(),
                to_version: "static-v2".to_string(),
                total_events: 50_000,
            },
            SyncEvent::PricingUpgradeProgress {
                from_version: "static-v1".to_string(),
                to_version: "static-v2".to_string(),
                processed_events: 25_000,
                total_events: 50_000,
                elapsed_ms: 1_234,
            },
            SyncEvent::PricingBucketReconcileStarted {
                to_version: "static-v2".to_string(),
                bucket_count: 7_153,
            },
            SyncEvent::PricingUpgradeFinished {
                from_version: "static-v1".to_string(),
                to_version: "static-v2".to_string(),
                updated_events: 539_146,
                bucket_count: 7_153,
                deleted_orphan_buckets: 2,
                elapsed_ms: 46_000,
            },
            SyncEvent::LockWaiting { timeout_ms: 30_000 },
            SyncEvent::LockAcquired { wait_ms: 3 },
            SyncEvent::SourceStarted {
                source: SourceKind::Codex,
                files_total: 10,
            },
            SyncEvent::Progress {
                source: SourceKind::Opencode,
                files_scanned: 50_000,
                records_imported: 12_345,
                current_file: Some("opencode.db".to_string()),
            },
            source_finished(SourceKind::Claude),
            SyncEvent::Failed {
                error: "boom".to_string(),
            },
            SyncEvent::Cancelled,
        ];
        for event in &events {
            if let Some(line) = human_progress_line(event) {
                assert!(!line.contains('\u{1b}'), "ESC in line for {event:?}");
            }
        }
    }

    #[test]
    fn human_progress_describes_pricing_upgrade_phases() {
        assert_eq!(
            human_progress_line(&SyncEvent::BootstrapStarted).as_deref(),
            Some("检查数据库 schema 与定价目录...")
        );
        assert_eq!(
            human_progress_line(&SyncEvent::PricingUpgradeProgress {
                from_version: "static-v1".to_string(),
                to_version: "static-v2".to_string(),
                processed_events: 25_000,
                total_events: 50_000,
                elapsed_ms: 1_234,
            })
            .as_deref(),
            Some("升级定价目录 static-v1 → static-v2：已处理 25000/50000 条（50.0%），已用 1234ms")
        );
        assert_eq!(
            human_progress_line(&SyncEvent::PricingBucketReconcileStarted {
                to_version: "static-v2".to_string(),
                bucket_count: 7_153,
            })
            .as_deref(),
            Some("事件定价完成，正在对账 7153 个 static-v2 汇总桶...")
        );
        assert_eq!(
            human_progress_line(&SyncEvent::PricingUpgradeFinished {
                from_version: "static-v1".to_string(),
                to_version: "static-v2".to_string(),
                updated_events: 539_146,
                bucket_count: 7_153,
                deleted_orphan_buckets: 2,
                elapsed_ms: 46_000,
            })
            .as_deref(),
            Some(
                "定价目录 static-v2 升级完成：539146 条事件，7153 个桶，清理 2 个孤立桶（46000ms）"
            )
        );
    }

    #[test]
    fn pricing_sync_event_serializes_as_additive_snake_case_ndjson() -> anyhow::Result<()> {
        let value = serde_json::to_value(SyncEvent::PricingUpgradeProgress {
            from_version: "static-v1".to_string(),
            to_version: "static-v2".to_string(),
            processed_events: 25_000,
            total_events: 50_000,
            elapsed_ms: 1_234,
        })?;

        assert_eq!(value["event"], "pricing_upgrade_progress");
        assert_eq!(value["processed_events"], 25_000);
        assert_eq!(value["total_events"], 50_000);
        Ok(())
    }
}
