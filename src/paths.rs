use std::path::PathBuf;

use anyhow::Result;

use crate::util::resolve_home_dir;

/// Concrete on-disk layout used by the local-only runtime.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Root runtime directory, typically `~/.llmusage`.
    pub root_dir: PathBuf,
    /// SQLite database path.
    pub db_path: PathBuf,
    /// Generated wrapper script directory.
    pub bin_dir: PathBuf,
    /// Backup directory for mutated third-party configs.
    pub backups_dir: PathBuf,
    /// Default export output directory.
    pub exports_dir: PathBuf,
    /// Windows hook wrapper path.
    pub hook_cmd_path: PathBuf,
    /// POSIX hook wrapper path.
    pub hook_sh_path: PathBuf,
    /// Legacy lock file path kept for compatibility/debugging.
    pub lock_path: PathBuf,
}

impl AppPaths {
    /// Builds the runtime layout under the current user's home directory.
    pub fn discover() -> Result<Self> {
        let home_dir = resolve_home_dir();
        if home_dir.as_os_str().is_empty() {
            anyhow::bail!("无法解析用户主目录");
        }
        let root_dir = home_dir.join(".llmusage");
        let bin_dir = root_dir.join("bin");
        let backups_dir = root_dir.join("backups");
        let exports_dir = root_dir.join("exports");

        Ok(Self {
            db_path: root_dir.join("llmusage.db"),
            hook_cmd_path: bin_dir.join("llmusage-hook.cmd"),
            hook_sh_path: bin_dir.join("llmusage-hook.sh"),
            lock_path: root_dir.join("worker.lock"),
            root_dir,
            bin_dir,
            backups_dir,
            exports_dir,
        })
    }
}
