use std::path::PathBuf;

use crate::{error::Result, util::resolve_home_dir};

/// Concrete on-disk layout used by the local-only runtime.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Root runtime directory, typically `~/.llmusage`.
    pub root_dir: PathBuf,
    /// SQLite database path.
    pub db_path: PathBuf,
    /// Generated wrapper script directory.
    pub bin_dir: PathBuf,
    /// Backup directory for mutated third-party configs and DB upgrades.
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
    /// Builds the runtime layout under `LLMUSAGE_HOME` when present, otherwise
    /// under the current user's home directory (`~/.llmusage`).
    pub fn discover() -> Result<Self> {
        if let Some(root_dir) = env_root("LLMUSAGE_HOME") {
            return Self::with_root(root_dir);
        }
        Self::with_cli_home(None)
    }

    /// Builds the runtime layout for an explicit llmusage root directory.
    ///
    /// This bypasses `LLMUSAGE_HOME` and is the preferred entrypoint for tests
    /// and Tauri adapters that need per-profile isolation.
    pub fn with_root(root_dir: PathBuf) -> Result<Self> {
        Ok(Self::from_root(root_dir))
    }

    /// Builds the runtime layout from an optional CLI home override.
    ///
    /// `Some(path)` is interpreted as the llmusage root itself. `None` falls
    /// back to `~/.llmusage` and deliberately does not read `LLMUSAGE_HOME`;
    /// callers that want env discovery should use [`Self::discover`].
    pub fn with_cli_home(home: Option<PathBuf>) -> Result<Self> {
        match home {
            Some(root_dir) => Self::with_root(root_dir),
            None => {
                let home_dir = resolve_home_dir();
                if home_dir.as_os_str().is_empty() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "无法解析用户主目录",
                    )
                    .into());
                }
                Self::with_root(home_dir.join(".llmusage"))
            }
        }
    }

    fn from_root(root_dir: PathBuf) -> Self {
        let bin_dir = root_dir.join("bin");
        let backups_dir = root_dir.join("backups");
        let exports_dir = root_dir.join("exports");

        Self {
            db_path: root_dir.join("llmusage.db"),
            hook_cmd_path: bin_dir.join("llmusage-hook.cmd"),
            hook_sh_path: bin_dir.join("llmusage-hook.sh"),
            lock_path: root_dir.join("worker.lock"),
            root_dir,
            bin_dir,
            backups_dir,
            exports_dir,
        }
    }
}

fn env_root(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn discover_uses_env_when_set() -> anyhow::Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = TempDir::new()?;
        let saved = std::env::var_os("LLMUSAGE_HOME");
        unsafe { std::env::set_var("LLMUSAGE_HOME", temp.path()) };

        let paths = AppPaths::discover()?;
        assert_eq!(paths.root_dir, temp.path());
        assert_eq!(paths.db_path, temp.path().join("llmusage.db"));

        restore_env("LLMUSAGE_HOME", saved);
        Ok(())
    }

    #[test]
    fn with_root_overrides_env() -> anyhow::Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        let env_root = TempDir::new()?;
        let explicit_root = TempDir::new()?;
        let saved = std::env::var_os("LLMUSAGE_HOME");
        unsafe { std::env::set_var("LLMUSAGE_HOME", env_root.path()) };

        let paths = AppPaths::with_root(explicit_root.path().to_path_buf())?;
        assert_eq!(paths.root_dir, explicit_root.path());
        assert_ne!(paths.root_dir, env_root.path());

        restore_env("LLMUSAGE_HOME", saved);
        Ok(())
    }

    fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
        unsafe {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}
