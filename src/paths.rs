use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root_dir: PathBuf,
    pub db_path: PathBuf,
    pub bin_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub exports_dir: PathBuf,
    pub hook_cmd_path: PathBuf,
    pub hook_sh_path: PathBuf,
    pub lock_path: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let home_dir = dirs::home_dir().context("无法解析用户主目录")?;
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
