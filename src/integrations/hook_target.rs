use std::path::{Path, PathBuf};

use crate::{app::AppContext, models::SourceKind};

/// Target shell environment for the generated llmusage hook wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookKind {
    /// Windows `cmd /c` wrapper backed by a `.cmd` script.
    WindowsCmd,
    /// Unix `/usr/bin/env sh` wrapper backed by a `.sh` script.
    UnixSh,
}

/// Adapter that owns the single `cfg!(windows)` branch for hook wiring.
///
/// All callers must build the platform-specific shell command / notify args
/// through this adapter — direct platform branches in callers are forbidden.
#[derive(Debug, Clone)]
pub struct HookTarget {
    kind: HookKind,
    path: PathBuf,
}

impl HookTarget {
    /// Build the current platform's hook target.
    ///
    /// 唯一聚合 `cfg!(windows)` 的入口；所有调用方都必须从这里拿目标。
    pub fn current(app: &AppContext) -> Self {
        if cfg!(windows) {
            Self {
                kind: HookKind::WindowsCmd,
                path: app.paths.hook_cmd_path.clone(),
            }
        } else {
            Self {
                kind: HookKind::UnixSh,
                path: app.paths.hook_sh_path.clone(),
            }
        }
    }

    pub fn kind(&self) -> HookKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Build a single-string shell command suitable for embedding in a
    /// foreign tool's settings file (Claude `hooks[*].command`,
    /// OpenCode plugin `$\`...\``, …).
    pub fn shell_command(&self, source: SourceKind, trigger: &str) -> String {
        match self.kind {
            HookKind::WindowsCmd => format!(
                "cmd /c \"{} --source {} --trigger {} --auto\"",
                quote_windows_cmd_path(&self.path),
                source.as_str(),
                trigger
            ),
            HookKind::UnixSh => format!(
                "/usr/bin/env sh {} --source {} --trigger {} --auto",
                quote_unix_path(&self.path),
                source.as_str(),
                trigger
            ),
        }
    }

    /// Build an argv vector suitable for `notify`-style integrations
    /// (Codex `notify` array).
    pub fn notify_args(&self, source: SourceKind, trigger: &str) -> Vec<String> {
        match self.kind {
            HookKind::WindowsCmd => vec![
                "cmd".to_string(),
                "/c".to_string(),
                self.path.to_string_lossy().to_string(),
                "--source".to_string(),
                source.as_str().to_string(),
                "--trigger".to_string(),
                trigger.to_string(),
                "--auto".to_string(),
            ],
            HookKind::UnixSh => vec![
                "/usr/bin/env".to_string(),
                "sh".to_string(),
                self.path.to_string_lossy().to_string(),
                "--source".to_string(),
                source.as_str().to_string(),
                "--trigger".to_string(),
                trigger.to_string(),
                "--auto".to_string(),
            ],
        }
    }
}

fn quote_unix_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if raw
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "/._-".contains(ch))
    {
        raw.to_string()
    } else {
        format!("\"{}\"", raw.replace('"', "\\\""))
    }
}

fn quote_windows_cmd_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('"', "\"\""))
}
