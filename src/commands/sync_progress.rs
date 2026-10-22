//! Human-mode progress rendering for `llmusage sync`.
//!
//! One event entry point and one copy source (`human_progress_line`) feed two
//! renderers: `LineRenderer` keeps the legacy line-by-line stderr behavior for
//! non-TTY output and `LLMUSAGE_PROGRESS=off`, while `BarRenderer` draws
//! indicatif bars on a TTY. `TerminalGuard` gives the command layer RAII
//! cleanup so every exit path (early `?`, failure, cancel, success) leaves no
//! dangling bar behind.

use std::{
    collections::HashMap,
    ffi::OsString,
    io::{IsTerminal, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::common::i18n::{self, Language};
use crate::{models::SourceKind, parsers::SyncEvent};

const SPINNER_TICK: Duration = Duration::from_millis(100);
const DRAW_HZ: u8 = 10;
const MAX_FILE_LABEL_CHARS: usize = 40;
/// An uncontended lock acquisition shorter than this prints no permanent line.
const LOCK_WAIT_NOTICE_MS: u64 = 1_000;

/// Production renderer for the CLI: indicatif bars when stderr is a TTY and
/// `LLMUSAGE_PROGRESS` is unset/empty, plain lines otherwise.
pub(crate) fn stderr_renderer() -> HumanRenderer {
    let draw = if std::io::stderr().is_terminal() {
        ProgressDrawTarget::stderr_with_hz(DRAW_HZ)
    } else {
        ProgressDrawTarget::hidden()
    };
    HumanRenderer::new(draw, progress_env_forces_line() || colors_disabled())
}

fn colors_disabled() -> bool {
    std::env::var_os("NO_COLOR").is_some() || std::env::var_os("LLMUSAGE_NO_COLOR").is_some()
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
    pub(crate) fn observe_notice(&mut self, notice: &crate::sync::notices::RunNotice) {
        if let crate::sync::notices::RunNotice::RemoteSkipped {
            host_id,
            facts: Some(facts),
            ..
        } = notice
        {
            let pending = match self {
                Self::Line(renderer) => &mut renderer.remote_facts,
                Self::Bar(renderer) => &mut renderer.remote_facts,
            };
            pending.insert(host_id.clone(), facts.as_ref().clone());
        }
    }

    pub(crate) fn suspend<T>(&mut self, operation: impl FnOnce() -> T) -> T {
        match self {
            Self::Line(renderer) => {
                renderer.finish();
                operation()
            }
            Self::Bar(renderer) => renderer.multi.suspend(operation),
        }
    }
    pub(crate) fn render_failure(&mut self, error: &anyhow::Error) {
        let language = match self {
            Self::Line(renderer) => renderer.language,
            Self::Bar(renderer) => renderer.language,
        };
        let detail = error
            .downcast_ref::<crate::sync::failure::SyncFailure>()
            .map(|failure| crate::common::i18n::sync::failure(&failure.kind, language))
            .unwrap_or_else(|| error.to_string());
        let line = crate::common::i18n::logging::safe_text(&format!(
            "{}: {detail}",
            language.select("Sync failed", "同步失败")
        ));
        match self {
            Self::Line(renderer) => {
                renderer.finish();
                renderer.terminated = true;
                let _ = writeln!(renderer.stderr, "{line}");
            }
            Self::Bar(renderer) => {
                renderer.clear_active();
                renderer.terminated = true;
                renderer.permanent_text(line);
            }
        }
    }

    pub(crate) fn write_warning(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Line(renderer) => {
                if renderer.last_line_len > 0 {
                    writeln!(renderer.stderr)?;
                    renderer.last_line_len = 0;
                }
                renderer.stderr.write_all(bytes)
            }
            Self::Bar(renderer) => renderer.multi.suspend(|| renderer.stderr.write_all(bytes)),
        }
    }

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
        let language = match self {
            Self::Line(renderer) => renderer.language,
            Self::Bar(renderer) => renderer.language,
        };
        i18n::with_language(language, || match self {
            Self::Line(renderer) => renderer.render(event),
            Self::Bar(renderer) => renderer.render(event),
        });
        if let SyncEvent::RemoteHostSkipped { host_id, .. } = event {
            match self {
                Self::Line(renderer) => {
                    renderer.remote_facts.remove(host_id);
                }
                Self::Bar(renderer) => {
                    renderer.remote_facts.remove(host_id);
                }
            }
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

/// How a startup event reaches the terminal after [`StartupDigest::fold`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fold {
    /// Normal rendering.
    Pass,
    /// Live TTY status only; never a permanent or non-TTY line.
    Transient,
    /// Not rendered at all.
    Hidden,
}

/// Folds startup chatter shared by both renderers: bootstrap and lock waits
/// are live status only, per-migration lines become one summary line, and a
/// lock acquired in under [`LOCK_WAIT_NOTICE_MS`] prints nothing. The engine
/// emits `LockWaiting` on every run, so it does not mean contention.
#[derive(Debug, Default)]
struct StartupDigest {
    migrations: u32,
    latest_version: u32,
    elapsed_ms: u64,
}

impl StartupDigest {
    fn fold(&mut self, event: &SyncEvent) -> Fold {
        match event {
            SyncEvent::MigrationStarted { latest_version, .. } => {
                self.latest_version = *latest_version;
                Fold::Transient
            }
            SyncEvent::MigrationFinished { elapsed_ms, .. } => {
                self.migrations = self.migrations.saturating_add(1);
                self.elapsed_ms = self.elapsed_ms.saturating_add(*elapsed_ms);
                Fold::Hidden
            }
            SyncEvent::BootstrapStarted | SyncEvent::LockWaiting { .. } => Fold::Transient,
            SyncEvent::LockAcquired { wait_ms } => {
                if *wait_ms >= LOCK_WAIT_NOTICE_MS {
                    Fold::Pass
                } else {
                    Fold::Hidden
                }
            }
            _ => Fold::Pass,
        }
    }

    /// Takes the pending migration summary once the migration phase is over.
    fn take_summary(&mut self, event: Option<&SyncEvent>, language: Language) -> Option<String> {
        let migrating = matches!(
            event,
            Some(SyncEvent::MigrationStarted { .. } | SyncEvent::MigrationFinished { .. })
        );
        if migrating || self.migrations == 0 {
            return None;
        }
        let line = crate::common::i18n::sync::migration_summary(
            self.migrations,
            self.latest_version,
            self.elapsed_ms,
            language,
        );
        self.migrations = 0;
        self.elapsed_ms = 0;
        Some(line)
    }
}

/// Success mark for permanent phase lines. Warning copy already starts with
/// "Warning:" / "警告：", so it carries no extra mark.
fn is_done_line(event: &SyncEvent) -> bool {
    matches!(
        event,
        SyncEvent::PricingBucketReconcileStarted { .. }
            | SyncEvent::PricingUpgradeFinished { .. }
            | SyncEvent::LockAcquired { .. }
            | SyncEvent::TokenAccountingRepairFinished { .. }
            | SyncEvent::RemoteHostFinished { .. }
    )
}

fn done_mark(line: &str, color: bool) -> String {
    if color {
        let mark = console::Style::new()
            .green()
            .force_styling(true)
            .apply_to("✓");
        format!("{mark} {line}")
    } else {
        format!("✓ {line}")
    }
}

/// Legacy line-by-line stderr renderer (former `HumanProgress`), preserved
/// verbatim for non-TTY output and the `LLMUSAGE_PROGRESS=off` fallback.
pub(crate) struct LineRenderer {
    language: Language,
    stderr: Box<dyn Write + Send>,
    tty: bool,
    last_line_len: usize,
    terminated: bool,
    remote_facts: HashMap<String, crate::remote::diagnostic::DiagnosticFacts>,
    digest: StartupDigest,
}

impl LineRenderer {
    pub(crate) fn new() -> Self {
        let stderr = std::io::stderr();
        let tty = stderr.is_terminal() && !progress_env_forces_line() && !colors_disabled();
        Self {
            language: i18n::current(),
            stderr: Box::new(stderr),
            tty,
            last_line_len: 0,
            terminated: false,
            remote_facts: HashMap::new(),
            digest: StartupDigest::default(),
        }
    }

    fn render(&mut self, event: &SyncEvent) {
        if self.terminated {
            return;
        }
        let fold = self.digest.fold(event);
        if let Some(summary) = self.digest.take_summary(Some(event), self.language) {
            self.write_permanent(&done_mark(&summary, false));
        }
        if fold == Fold::Hidden || (fold == Fold::Transient && !self.tty) {
            return;
        }
        if matches!(event, SyncEvent::Failed { .. } | SyncEvent::Cancelled) {
            self.terminated = true;
        }
        if matches!(event, SyncEvent::SourceFinished { .. }) {
            // Erase the active TTY line instead of preserving the final progress
            // snapshot; the stdout summary is the only permanent success view.
            if self.tty && self.last_line_len > 0 {
                let _ = write!(self.stderr, "\r{}\r", " ".repeat(self.last_line_len));
                let _ = self.stderr.flush();
                self.last_line_len = 0;
            }
            return;
        }
        let Some(mut line) = self.progress_line(event) else {
            return;
        };
        if is_done_line(event) {
            line = done_mark(&line, false);
        }
        if self.tty {
            let padding = self
                .last_line_len
                .saturating_sub(unicode_width::UnicodeWidthStr::width(line.as_str()));
            let _ = write!(self.stderr, "\r{line}{}", " ".repeat(padding));
            let _ = self.stderr.flush();
            self.last_line_len = unicode_width::UnicodeWidthStr::width(line.as_str());
            if matches!(
                event,
                SyncEvent::PricingBucketReconcileStarted { .. }
                    | SyncEvent::PricingUpgradeFinished { .. }
                    | SyncEvent::LockAcquired { .. }
                    | SyncEvent::TokenAccountingRepairStarted { .. }
                    | SyncEvent::TokenAccountingRepairFinished { .. }
                    | SyncEvent::RemoteHostFinished { .. }
                    | SyncEvent::RemoteHostSkipped { .. }
            ) {
                let _ = writeln!(self.stderr);
                self.last_line_len = 0;
            }
        } else {
            let _ = writeln!(self.stderr, "{line}");
        }
    }

    /// Lands one permanent line, overwriting any live TTY status line.
    fn write_permanent(&mut self, line: &str) {
        if self.tty && self.last_line_len > 0 {
            let padding = self
                .last_line_len
                .saturating_sub(unicode_width::UnicodeWidthStr::width(line));
            let _ = writeln!(self.stderr, "\r{line}{}", " ".repeat(padding));
            self.last_line_len = 0;
        } else {
            let _ = writeln!(self.stderr, "{line}");
        }
        let _ = self.stderr.flush();
    }

    fn finish(&mut self) {
        if let Some(summary) = self.digest.take_summary(None, self.language) {
            self.write_permanent(&done_mark(&summary, false));
        }
        if self.last_line_len > 0 {
            let _ = writeln!(self.stderr);
            self.last_line_len = 0;
        }
        let _ = self.stderr.flush();
    }

    fn progress_line(&self, event: &SyncEvent) -> Option<String> {
        let facts = match event {
            SyncEvent::RemoteHostSkipped { host_id, .. } => self.remote_facts.get(host_id),
            _ => None,
        };
        crate::common::i18n::sync::progress_with_remote(event, facts, self.language)
            .map(|line| crate::common::i18n::logging::safe_text(&line))
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
    language: Language,
    multi: MultiProgress,
    stderr: Box<dyn Write + Send>,
    active: Option<ActiveBar>,
    terminated: bool,
    remote_facts: HashMap<String, crate::remote::diagnostic::DiagnosticFacts>,
    digest: StartupDigest,
}

impl BarRenderer {
    pub(crate) fn new(draw: ProgressDrawTarget) -> Self {
        Self {
            language: i18n::current(),
            multi: MultiProgress::with_draw_target(draw),
            stderr: Box::new(std::io::stderr()),
            active: None,
            terminated: false,
            remote_facts: HashMap::new(),
            digest: StartupDigest::default(),
        }
    }

    fn render(&mut self, event: &SyncEvent) {
        if self.terminated {
            return;
        }
        let fold = self.digest.fold(event);
        if let Some(summary) = self.digest.take_summary(Some(event), self.language) {
            self.complete_active();
            self.permanent_text(done_mark(&summary, true));
        }
        if fold == Fold::Hidden {
            return;
        }
        match event {
            SyncEvent::BootstrapStarted
            | SyncEvent::MigrationStarted { .. }
            | SyncEvent::LockWaiting { .. } => {
                self.start_spinner(self.progress_line(event).unwrap_or_default());
            }
            SyncEvent::PricingUpgradeStarted { total_events, .. } => {
                let line = self.progress_line(event).unwrap_or_default();
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
                    if let Some(line) = self.progress_line(event) {
                        active.bar.set_message(line);
                    }
                }
            }
            SyncEvent::PricingBucketReconcileStarted { .. } => {
                // Boundary: close the repricing bar with a permanent line,
                // then spin while buckets reconcile.
                self.permanent_line(event);
                self.start_spinner(self.progress_line(event).unwrap_or_default());
            }
            // Folded by `StartupDigest` above; never reaches here.
            SyncEvent::MigrationFinished { .. } => {}
            SyncEvent::PricingUpgradeFinished { .. }
            | SyncEvent::LockAcquired { .. }
            | SyncEvent::TokenAccountingRepairStarted { .. }
            | SyncEvent::TokenAccountingRepairFinished { .. } => {
                self.permanent_line(event);
            }
            SyncEvent::SourceFinished { .. } => {
                // Close the active bar without a permanent success line: the
                // final stdout summary table is the single success surface.
                self.clear_active();
            }
            SyncEvent::SourceStarted {
                source,
                files_total,
            } => {
                let line = self.progress_line(event).unwrap_or_default();
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
                let line = self.progress_line(event).unwrap_or_default();
                if let Some(active) = self.active.take() {
                    active.bar.abandon_with_message(line.clone());
                }
                self.permanent_text(line);
            }
            SyncEvent::RemoteHostStarted { .. } => {
                self.start_spinner(self.progress_line(event).unwrap_or_default());
            }
            SyncEvent::RemoteHostFinished { .. } | SyncEvent::RemoteHostSkipped { .. } => {
                self.permanent_line(event);
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
            let message = crate::common::i18n::sync::replay_message(
                source,
                pos,
                len,
                records_imported,
                i18n::current(),
            );
            active.bar.set_message(message);
        } else {
            // OpenCode: the spinner position carries scanned row counts.
            active.bar.set_position(files_scanned);
            let file = current_file.map(short_file_label).unwrap_or_default();
            let message = if file.is_empty() {
                format!(
                    "{} {records_imported} {}",
                    i18n::text("Imported", "导入"),
                    i18n::text("records", "条")
                )
            } else {
                format!(
                    "{} {records_imported} {} · {file}",
                    i18n::text("Imported", "导入"),
                    i18n::text("records", "条")
                )
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
        if let Some(line) = self.progress_line(event) {
            let line = if is_done_line(event) {
                done_mark(&line, true)
            } else {
                line
            };
            self.permanent_text(line);
        }
    }

    fn permanent_text(&mut self, line: String) {
        // indicatif println pads to the terminal width instead of writing a
        // newline. A following raw warning then shares the same output line.
        let _ = self.multi.suspend(|| writeln!(self.stderr, "{line}"));
    }

    fn finish(&mut self) {
        if let Some(summary) = self.digest.take_summary(None, self.language) {
            self.complete_active();
            self.permanent_text(done_mark(&summary, true));
        }
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

    #[cfg(test)]
    fn active_message(&self) -> Option<String> {
        self.active.as_ref().map(|active| active.bar.message())
    }

    fn progress_line(&self, event: &SyncEvent) -> Option<String> {
        let facts = match event {
            SyncEvent::RemoteHostSkipped { host_id, .. } => self.remote_facts.get(host_id),
            _ => None,
        };
        crate::common::i18n::sync::progress_with_remote(event, facts, self.language)
            .map(|line| crate::common::i18n::logging::safe_text(&line))
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
    let name = crate::common::i18n::logging::safe_text(name);
    if name.chars().count() > MAX_FILE_LABEL_CHARS {
        let mut shortened: String = name.chars().take(MAX_FILE_LABEL_CHARS - 1).collect();
        shortened.push('…');
        shortened
    } else {
        name.to_string()
    }
}

#[cfg(test)]
pub(crate) fn human_progress_line(event: &SyncEvent) -> Option<String> {
    crate::common::i18n::sync::progress(event, i18n::current())
        .map(|line| crate::common::i18n::logging::safe_text(&line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_remote_facts_reach_line_bar_and_summary_without_changing_events() {
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                let facts = crate::remote::diagnostic::DiagnosticFacts::Protocol {
                    remote: 99,
                    schema: 42,
                };
                let notice = crate::sync::notices::RunNotice::RemoteSkipped {
                    host_id: "devbox".into(),
                    label: "My host".into(),
                    detail: "CANONICAL_REASON".into(),
                    facts: Some(Box::new(facts)),
                };
                let event = SyncEvent::RemoteHostSkipped {
                    host_id: "devbox".into(),
                    label: "My host".into(),
                    reason: "CANONICAL_REASON".into(),
                };
                let canonical = serde_json::to_string(&event).unwrap();
                for bar in [false, true] {
                    let capture = CaptureTerm::default();
                    let mut renderer = if bar {
                        let mut renderer = BarRenderer::new(ProgressDrawTarget::term_like(
                            Box::new(capture.clone()),
                        ));
                        renderer.stderr = Box::new(capture.clone());
                        HumanRenderer::Bar(renderer)
                    } else {
                        HumanRenderer::Line(LineRenderer {
                            language,
                            stderr: Box::new(capture.clone()),
                            tty: false,
                            last_line_len: 0,
                            terminated: false,
                            remote_facts: HashMap::new(),
                            digest: StartupDigest::default(),
                        })
                    };
                    renderer.observe_notice(&notice);
                    renderer.render(&event);
                    renderer.finish();
                    let output = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
                    let expected = language.select("shard protocol mismatch", "分片协议不一致");
                    assert!(output.contains(expected), "{output}");
                    assert!(output.contains("99") && output.contains("42"));
                    assert!(!output.contains("CANONICAL_REASON"));
                    let pending = match renderer {
                        HumanRenderer::Line(renderer) => renderer.remote_facts.len(),
                        HumanRenderer::Bar(renderer) => renderer.remote_facts.len(),
                    };
                    assert_eq!(pending, 0, "facts are consumed with their own host event");
                    let summary =
                        crate::commands::sync_summary::notice_lines(&notice, false).join("\n");
                    assert!(summary.contains(expected), "{summary}");
                }
                assert_eq!(serde_json::to_string(&event).unwrap(), canonical);
                let unknown =
                    crate::common::i18n::sync::progress_with_remote(&event, None, language)
                        .unwrap();
                assert!(unknown.contains("CANONICAL_REASON"));
                assert!(unknown.contains(language.select("Original detail", "原始详情")));
            });
        }
    }

    #[tokio::test]
    async fn reporter_keeps_the_renderer_language_snapshot_across_spawn() {
        let bytes = CaptureTerm::default();
        let renderer = i18n::with_language(Language::Chinese, || {
            Arc::new(Mutex::new(HumanRenderer::Line(LineRenderer {
                language: i18n::current(),
                stderr: Box::new(bytes.clone()),
                // Bootstrap status is live TTY output only.
                tty: true,
                last_line_len: 0,
                terminated: false,
                remote_facts: HashMap::new(),
                digest: StartupDigest::default(),
            })))
        });
        tokio::spawn(async move {
            assert_eq!(i18n::current(), Language::English);
            render_shared(&renderer, &SyncEvent::BootstrapStarted);
        })
        .await
        .unwrap();
        let output = String::from_utf8(bytes.0.lock().unwrap().clone()).unwrap();
        assert!(output.contains("检查数据库") && !output.contains("Checking"));
    }

    #[test]
    fn bilingual_progress_preserves_counts_shortens_only_version_identity_and_wire_is_unchanged() {
        let event = SyncEvent::PricingUpgradeProgress {
            from_version: "public-0123456789abcdef0123456789abcdef".into(),
            to_version: "public-fedcba9876543210fedcba9876543210".into(),
            processed_events: 70000,
            total_events: 399560,
            elapsed_ms: 31100,
        };
        let before = serde_json::to_value(&event).unwrap();
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                let line = human_progress_line(&event).unwrap();
                assert!(
                    line.contains("70000/399560")
                        && line.contains("17.5%")
                        && line.contains("31100ms")
                );
                assert!(!line.contains("0123456789abcdef0123456789abcdef"));
                assert_eq!(serde_json::to_value(&event).unwrap(), before);
                assert_eq!(
                    human_progress_line(&SyncEvent::Cancelled).unwrap(),
                    language.select("Sync cancelled", "同步已取消")
                );
                let replay =
                    crate::common::i18n::sync::replay_message(SourceKind::Codex, 2, 2, 5, language);
                assert!(
                    replay.contains(language.select("committing", "正在提交"))
                        && replay.contains('5')
                );
            });
        }
    }
    use crate::parsers::SourceSyncStats;

    #[derive(Clone, Debug, Default)]
    struct CaptureTerm(Arc<Mutex<Vec<u8>>>);

    impl Write for CaptureTerm {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl indicatif::TermLike for CaptureTerm {
        fn width(&self) -> u16 {
            120
        }
        fn move_cursor_up(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn move_cursor_down(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn move_cursor_right(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn move_cursor_left(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn write_line(&self, s: &str) -> std::io::Result<()> {
            writeln!(self.clone(), "{s}")
        }
        fn write_str(&self, s: &str) -> std::io::Result<()> {
            self.clone().write_all(s.as_bytes())
        }
        fn clear_line(&self) -> std::io::Result<()> {
            self.clone().write_all(b"\r")
        }
        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn permanent_progress_lines_end_before_raw_warnings() {
        i18n::with_language(Language::Chinese, || {
            let terminal = CaptureTerm::default();
            let mut renderer =
                BarRenderer::new(ProgressDrawTarget::term_like(Box::new(terminal.clone())));
            renderer.stderr = Box::new(terminal.clone());
            renderer.render(&SyncEvent::LockAcquired { wait_ms: 1_500 });
            let writer = terminal.clone();
            let subscriber = tracing_subscriber::fmt()
                .without_time()
                .with_ansi(false)
                .with_target(false)
                .with_writer(move || writer.clone())
                .finish();
            tracing::subscriber::with_default(subscriber, || tracing::warn!("legacy warning"));
            let text = String::from_utf8(terminal.0.lock().unwrap().clone()).unwrap();
            let warning = text.find("WARN").expect("warning");
            assert!(
                text[..warning].trim_end_matches(' ').ends_with('\n'),
                "{text:?}"
            );
        })
    }

    #[test]
    fn scoped_warning_sink_handles_interleaved_progress_and_restores_on_error() {
        i18n::with_language(Language::Chinese, || {
            for mode in ["bar", "tty_line", "non_tty"] {
                let terminal = CaptureTerm::default();
                let renderer = if mode == "bar" {
                    let mut bar =
                        BarRenderer::new(ProgressDrawTarget::term_like(Box::new(terminal.clone())));
                    bar.stderr = Box::new(terminal.clone());
                    HumanRenderer::Bar(bar)
                } else {
                    HumanRenderer::Line(LineRenderer {
                        language: i18n::current(),
                        stderr: Box::new(terminal.clone()),
                        tty: mode == "tty_line",
                        last_line_len: 0,
                        terminated: false,
                        remote_facts: HashMap::new(),
                        digest: StartupDigest::default(),
                    })
                };
                let renderer = Arc::new(Mutex::new(renderer));
                let outside = CaptureTerm::default();
                let outside_sink = outside.clone();
                let _outer = crate::logging::install_stderr_sink(Arc::new(move |bytes| {
                    outside_sink.clone().write_all(bytes)
                }));
                let subscriber = tracing_subscriber::fmt()
                    .without_time()
                    .with_ansi(false)
                    .with_target(false)
                    .with_writer(crate::logging::StderrLogWriter::default)
                    .finish();
                tracing::subscriber::with_default(subscriber, || {
                    let result: std::io::Result<()> = {
                        let _terminal_guard = TerminalGuard::new(Arc::clone(&renderer));
                        let sink_renderer = Arc::clone(&renderer);
                        let _warning_guard =
                            crate::logging::install_stderr_sink(Arc::new(move |bytes| {
                                sink_renderer.lock().unwrap().write_warning(bytes)
                            }));
                        render_shared(&renderer, &SyncEvent::LockAcquired { wait_ms: 58 });
                        tracing::warn!("first warning");
                        crate::logging::stderr_warning("second warning");
                        render_shared(
                            &renderer,
                            &SyncEvent::SourceStarted {
                                source: SourceKind::Codex,
                                files_total: 2,
                            },
                        );
                        tracing::warn!("interleaved warning");
                        render_shared(&renderer, &SyncEvent::LockAcquired { wait_ms: 0 });
                        tracing::warn!("last warning");
                        render_shared(
                            &renderer,
                            &SyncEvent::SourceStarted {
                                source: SourceKind::Codex,
                                files_total: 2,
                            },
                        );
                        Err(std::io::Error::other("fixture error"))
                    };
                    assert!(result.is_err());
                    tracing::warn!("outside warning");
                });
                assert!(!renderer.lock().unwrap().has_active_bar());
                let captured = String::from_utf8(terminal.0.lock().unwrap().clone()).unwrap();
                for message in [
                    "first warning",
                    "second warning",
                    "interleaved warning",
                    "last warning",
                ] {
                    assert!(
                        captured.contains(&format!("{message}\n")),
                        "{mode}: {captured:?}"
                    );
                }
                assert!(!captured.contains("outside warning"));
                if mode == "tty_line" {
                    assert!(
                        captured.ends_with('\n'),
                        "unfinished TTY line: {captured:?}"
                    );
                }
                let external = String::from_utf8(outside.0.lock().unwrap().clone()).unwrap();
                assert!(external.contains("outside warning"));
                assert!(!external.contains("interleaved warning"));
                if mode == "non_tty" {
                    assert!(!captured.contains('\r'));
                    assert_eq!(
                        captured
                            .lines()
                            .filter(|line| line.contains("warning"))
                            .count(),
                        4
                    );
                }
            }
        })
    }

    fn hidden_bar_renderer() -> BarRenderer {
        let mut renderer = BarRenderer::new(ProgressDrawTarget::hidden());
        renderer.stderr = Box::new(std::io::sink());
        renderer
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
    fn source_finished_emits_no_permanent_human_line() {
        i18n::with_language(Language::Chinese, || {
            // The duplicate per-source completion sentence is gone; the final
            // summary table owns the per-source success surface.
            assert!(human_progress_line(&source_finished(SourceKind::Codex)).is_none());
            assert!(human_progress_line(&source_finished(SourceKind::Opencode)).is_none());
        })
    }

    #[test]
    fn remote_host_events_render_skip_warnings() {
        i18n::with_language(Language::Chinese, || {
            assert_eq!(
                human_progress_line(&SyncEvent::RemoteHostStarted {
                    host_id: "devbox".to_string(),
                    label: "devbox".to_string(),
                })
                .as_deref(),
                Some("正在导入远端主机 devbox...")
            );
            assert_eq!(
                human_progress_line(&SyncEvent::RemoteHostSkipped {
                    host_id: "devbox".to_string(),
                    label: "devbox".to_string(),
                    reason: "ssh timed out".to_string(),
                })
                .as_deref(),
                Some("警告：跳过远端主机 devbox：ssh timed out")
            );
        })
    }

    #[test]
    fn bar_renderer_success_sequence_converges() {
        i18n::with_language(Language::Chinese, || {
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
            // The migration spinner stays live; its summary lands at the next phase.
            assert!(renderer.has_active());
            renderer.render(&SyncEvent::LockWaiting { timeout_ms: 30_000 });
            assert!(renderer.has_active());
            // A fast acquisition prints nothing; the next phase replaces the spinner.
            renderer.render(&SyncEvent::LockAcquired { wait_ms: 3 });
            assert!(renderer.has_active());

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
        })
    }

    #[test]
    fn determinate_source_progress_reports_replay_and_commit_phases() {
        i18n::with_language(Language::Chinese, || {
            let mut renderer = hidden_bar_renderer();
            let started = SyncEvent::SourceStarted {
                source: SourceKind::Codex,
                files_total: 3,
            };
            assert_eq!(
                human_progress_line(&started).as_deref(),
                Some("Codex: 本轮重放 3 个文件...")
            );
            assert_eq!(
                human_progress_line(&SyncEvent::SourceStarted {
                    source: SourceKind::Opencode,
                    files_total: 1,
                })
                .as_deref(),
                Some("OpenCode: 扫描 1 个文件...")
            );

            renderer.render(&started);
            renderer.render(&SyncEvent::Progress {
                source: SourceKind::Codex,
                files_scanned: 3,
                records_imported: 40,
                current_file: None,
            });
            assert_eq!(
                renderer.active_message().as_deref(),
                Some("Codex: 重放完成，正在提交... · 已导入 40 条")
            );
        })
    }

    #[test]
    fn bar_renderer_pricing_upgrade_sequence_converges() {
        i18n::with_language(Language::Chinese, || {
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
        })
    }

    #[test]
    fn bar_renderer_opencode_progress_never_overflows() {
        i18n::with_language(Language::Chinese, || {
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
        })
    }

    #[test]
    fn bar_renderer_failed_abandons_and_ignores_followups() {
        i18n::with_language(Language::Chinese, || {
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
        })
    }

    #[test]
    fn bar_renderer_cancelled_abandons_active_bar() {
        i18n::with_language(Language::Chinese, || {
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
        })
    }

    #[test]
    fn terminal_guard_finishes_renderer_on_drop() {
        i18n::with_language(Language::Chinese, || {
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
        })
    }

    #[test]
    fn selection_falls_back_to_line_renderer() {
        i18n::with_language(Language::Chinese, || {
            assert!(matches!(
                HumanRenderer::new(ProgressDrawTarget::hidden(), false),
                HumanRenderer::Line(_)
            ));
            assert!(matches!(
                HumanRenderer::new(ProgressDrawTarget::stderr_with_hz(10), true),
                HumanRenderer::Line(_)
            ));
        })
    }

    #[test]
    fn forced_line_requires_non_empty_env_value() {
        i18n::with_language(Language::Chinese, || {
            assert!(!forced_line(None));
            assert!(!forced_line(Some(OsString::new())));
            assert!(forced_line(Some(OsString::from("off"))));
            assert!(forced_line(Some(OsString::from("1"))));
        })
    }

    #[test]
    fn short_file_label_uses_last_segment_and_truncates() {
        i18n::with_language(Language::Chinese, || {
            assert_eq!(
                short_file_label("C:/data/opencode/opencode.db"),
                "opencode.db"
            );
            assert_eq!(short_file_label("state.db"), "state.db");
            let long = format!("{}\u{4e2d}.db", "a".repeat(80));
            let label = short_file_label(&long);
            assert_eq!(label.chars().count(), MAX_FILE_LABEL_CHARS);
            assert!(label.ends_with('…'));
        })
    }

    #[test]
    fn human_lines_never_contain_ansi_escapes() {
        i18n::with_language(Language::Chinese, || {
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
        })
    }

    #[test]
    fn dynamic_progress_detail_is_escaped_for_lines_and_bars_without_changing_events() {
        let events = [
            SyncEvent::RemoteHostSkipped {
                host_id: "remote".into(),
                label: "host\u{1b}[31m\nlabel".into(),
                reason: "failed\r\n\u{1b}[0m".into(),
            },
            SyncEvent::Failed {
                error: "denied\u{1b}[31m\nnext line".into(),
            },
        ];
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                for event in &events {
                    let before = serde_json::to_value(event).unwrap();
                    let expected = if matches!(event, SyncEvent::RemoteHostSkipped { .. }) {
                        language.select(
                            "Warning: skipped remote host host\\u{1b}[31m\\nlabel: Original detail: failed\\r\\n\\u{1b}[0m",
                            "警告：跳过远端主机 host\\u{1b}[31m\\nlabel：原始详情: failed\\r\\n\\u{1b}[0m",
                        ).to_string()
                    } else {
                        human_progress_line(event).unwrap()
                    };
                    assert!(!expected.chars().any(char::is_control), "{expected}");
                    assert!(expected.contains("\\u{1b}"), "{expected}");
                    let line_output = CaptureTerm::default();
                    let mut line = LineRenderer {
                        language,
                        stderr: Box::new(line_output.clone()),
                        tty: false,
                        last_line_len: 0,
                        terminated: false,
                        remote_facts: HashMap::new(),
                        digest: StartupDigest::default(),
                    };
                    line.render(event);
                    assert_eq!(
                        String::from_utf8(line_output.0.lock().unwrap().clone()).unwrap(),
                        format!("{expected}\n")
                    );
                    let bar_output = CaptureTerm::default();
                    let mut bar = BarRenderer::new(ProgressDrawTarget::hidden());
                    bar.stderr = Box::new(bar_output.clone());
                    bar.render(event);
                    assert_eq!(
                        String::from_utf8(bar_output.0.lock().unwrap().clone()).unwrap(),
                        format!("{expected}\n")
                    );
                    assert_eq!(serde_json::to_value(event).unwrap(), before);
                }
                let file = short_file_label("/private/file\u{1b}[31m\n.db");
                assert!(!file.chars().any(char::is_control));
            });
        }
    }

    #[test]
    fn human_progress_describes_pricing_upgrade_phases() {
        i18n::with_language(Language::Chinese, || {
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
                Some(
                    "升级定价目录 static-v1 → static-v2：已处理 25000/50000 条（50.0%），已用 1234ms"
                )
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
        })
    }

    #[test]
    fn pricing_sync_event_serializes_as_additive_snake_case_ndjson() -> anyhow::Result<()> {
        i18n::with_language(Language::Chinese, || {
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
        })
    }

    #[test]
    fn token_accounting_repair_events_have_actionable_copy_and_ndjson_shape() -> anyhow::Result<()>
    {
        i18n::with_language(Language::Chinese, || {
            let sources = vec![SourceKind::Codex, SourceKind::Claude];
            let started = SyncEvent::TokenAccountingRepairStarted {
                sources: sources.clone(),
            };
            let finished = SyncEvent::TokenAccountingRepairFinished { sources };

            assert_eq!(
                human_progress_line(&started).as_deref(),
                Some(
                    "警告：检测到旧版 token accounting（codex, claude）；本轮将自动执行无损重建，不会自动启用 --allow-lossy-rebuild。"
                )
            );
            assert_eq!(
                human_progress_line(&finished).as_deref(),
                Some("Token accounting 自动修复完成：codex, claude")
            );

            let started_json = serde_json::to_value(&started)?;
            let finished_json = serde_json::to_value(&finished)?;
            assert_eq!(started_json["event"], "token_accounting_repair_started");
            assert_eq!(finished_json["event"], "token_accounting_repair_finished");
            assert_eq!(
                started_json["sources"],
                serde_json::json!(["codex", "claude"])
            );
            assert_eq!(serde_json::from_value::<SyncEvent>(started_json)?, started);
            assert_eq!(
                serde_json::from_value::<SyncEvent>(finished_json)?,
                finished
            );
            Ok(())
        })
    }

    #[test]
    fn startup_migrations_fold_into_one_summary_and_fast_lock_is_silent() {
        for (language, expected) in [
            (
                Language::English,
                "✓ Database schema upgraded to v3 (3 migrations, 6ms)
",
            ),
            (
                Language::Chinese,
                "✓ 数据库 schema 已升级到 v3（3 个迁移，6ms）
",
            ),
        ] {
            let output = CaptureTerm::default();
            let mut line = LineRenderer {
                language,
                stderr: Box::new(output.clone()),
                tty: false,
                last_line_len: 0,
                terminated: false,
                remote_facts: HashMap::new(),
                digest: StartupDigest::default(),
            };
            for version in 1..=3 {
                line.render(&SyncEvent::MigrationStarted {
                    version,
                    name: format!("m{version}"),
                    latest_version: 3,
                });
                line.render(&SyncEvent::MigrationFinished {
                    version,
                    name: format!("m{version}"),
                    elapsed_ms: 2,
                });
            }
            line.render(&SyncEvent::LockAcquired { wait_ms: 35 });
            line.finish();
            assert_eq!(
                String::from_utf8(output.0.lock().unwrap().clone()).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn slow_lock_keeps_a_permanent_line() {
        let output = CaptureTerm::default();
        let mut line = LineRenderer {
            language: Language::English,
            stderr: Box::new(output.clone()),
            tty: false,
            last_line_len: 0,
            terminated: false,
            remote_facts: HashMap::new(),
            digest: StartupDigest::default(),
        };
        line.render(&SyncEvent::BootstrapStarted);
        line.render(&SyncEvent::LockWaiting { timeout_ms: 30_000 });
        line.render(&SyncEvent::LockAcquired { wait_ms: 40 });
        line.render(&SyncEvent::LockAcquired { wait_ms: 1_200 });
        assert_eq!(
            String::from_utf8(output.0.lock().unwrap().clone()).unwrap(),
            "✓ SQLite sync worker lock acquired (waited 1200ms)
"
        );
    }
}
