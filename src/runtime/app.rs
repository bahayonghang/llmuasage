use std::path::PathBuf;

use anyhow::Result;

use crate::paths::AppPaths;

/// Discovered runtime paths and process metadata shared by CLI commands.
#[derive(Debug, Clone)]
pub struct AppContext {
    /// Resolved runtime layout for the current user or explicit `--home` root.
    pub paths: AppPaths,
    /// Current executable path used when generating hook wrappers.
    pub current_exe: PathBuf,
}

impl AppContext {
    /// Discovers the current executable and all runtime directories.
    pub fn discover() -> Result<Self> {
        Self::with_cli_home(None)
    }

    /// Discovers process metadata while allowing a CLI-provided runtime root.
    pub fn with_cli_home(home: Option<PathBuf>) -> Result<Self> {
        let paths = match home {
            Some(root) => AppPaths::with_root(root)?,
            None => AppPaths::discover()?,
        };
        Self::from_paths(paths)
    }

    /// Builds context from a layout that has already been resolved.
    ///
    /// Unlike [`Self::with_cli_home`], this does not read `LLMUSAGE_HOME`.
    pub(crate) fn from_paths(paths: AppPaths) -> Result<Self> {
        Ok(Self {
            paths,
            current_exe: std::env::current_exe()?,
        })
    }
}
