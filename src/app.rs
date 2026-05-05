use std::path::PathBuf;

use anyhow::Result;

use crate::paths::AppPaths;

/// Discovered runtime paths and process metadata shared by CLI commands.
#[derive(Debug, Clone)]
pub struct AppContext {
    /// Resolved `~/.llmusage` runtime layout for the current user.
    pub paths: AppPaths,
    /// Current executable path used when generating hook wrappers.
    pub current_exe: PathBuf,
}

impl AppContext {
    /// Discovers the current executable and all runtime directories.
    pub fn discover() -> Result<Self> {
        Ok(Self {
            paths: AppPaths::discover()?,
            current_exe: std::env::current_exe()?,
        })
    }
}
