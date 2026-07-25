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

/// POSIX single-quote escaping.
///
/// Double quotes are NOT safe here: inside `"..."` a POSIX shell still expands
/// `$VAR`, `$(cmd)`, backticks and processes `\`. Single quotes make every byte
/// literal; the only character needing care is `'` itself, which is emitted by
/// closing the quote, adding an escaped `\'`, and reopening (`'\''`).
pub(crate) fn quote_unix_path(path: &Path) -> String {
    quote_posix(&path.to_string_lossy())
}

pub(crate) fn quote_posix(raw: &str) -> String {
    if !raw.is_empty()
        && raw
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "/._-".contains(ch))
    {
        return raw.to_string();
    }
    format!("'{}'", raw.replace('\'', r"'\''"))
}

fn quote_windows_cmd_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SEC-002: the old implementation wrapped paths in double quotes and only
    /// escaped `"`. A POSIX shell still expands `$(...)`, backticks, `${...}`
    /// and processes `\` inside double quotes, so a path containing those
    /// changed the meaning of the generated hook command.
    #[test]
    fn shell_metacharacters_stay_literal_in_posix_quoting() {
        let hostile = [
            "/home/u/$(touch /tmp/pwned)/llmusage-hook.sh",
            "/home/u/`id`/llmusage-hook.sh",
            "/home/u/${HOME}/llmusage-hook.sh",
            "/home/u/back\\slash/llmusage-hook.sh",
            "/home/u/with space/llmusage-hook.sh",
            "/home/u/\"quoted\"/llmusage-hook.sh",
        ];
        for raw in hostile {
            let quoted = quote_posix(raw);
            assert!(
                quoted.starts_with('\'') && quoted.ends_with('\''),
                "{raw} should be single-quoted, got {quoted}"
            );
            // no unescaped expansion characters may survive outside the quotes
            let inner = &quoted[1..quoted.len() - 1];
            assert!(
                !inner.contains('\'') || inner.contains(r"'\''"),
                "{raw}: single quotes must be escaped as '\\'', got {quoted}"
            );
        }
    }

    #[test]
    fn single_quote_in_path_is_escaped_posix_style() {
        // O'Brien is the classic case: naive single-quoting breaks out here.
        let quoted = quote_posix("/home/o'brien/hook.sh");
        assert_eq!(quoted, r"'/home/o'\''brien/hook.sh'");
    }

    #[test]
    fn plain_paths_are_not_quoted() {
        assert_eq!(
            quote_posix("/home/user/.llmusage/bin/llmusage-hook.sh"),
            "/home/user/.llmusage/bin/llmusage-hook.sh"
        );
    }

    #[test]
    fn empty_path_is_quoted_not_dropped() {
        assert_eq!(quote_posix(""), "''");
    }
}
