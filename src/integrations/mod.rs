use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use tracing::info;

use crate::{app::AppContext, models::SourceKind, registry, store::Store, util::now_utc};

pub mod antigravity;
mod atomic;
pub mod claude;
pub mod codex;
pub mod hook_target;
pub mod integration;
pub mod opencode;

pub use atomic::{remove_file_atomic_and_record, write_file_atomic, write_file_atomic_and_record};
pub use hook_target::{HookKind, HookTarget};
pub use integration::Integration;

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationProbe {
    pub source: SourceKind,
    pub status: String,
    pub detail: String,
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationAction {
    pub source: SourceKind,
    pub status: String,
    pub detail: String,
}

pub fn probe_all(app: &AppContext) -> Result<Vec<IntegrationProbe>> {
    registry::registered_integrations()
        .iter()
        .map(|integ| integ.probe(app))
        .collect()
}

pub fn install_all(app: &AppContext, store: &Store) -> Result<Vec<IntegrationAction>> {
    /*
     * ========================================================================
     * 步骤1：生成本地 hook 包装器并安装注册的所有集成
     * ========================================================================
     * 目标：
     * 1) 先生成 Windows / Unix 两类 hook 包装器
     * 2) 再按 registry::registered_integrations 顺序遍历安装
     * 3) 每个集成的安装结果都写入 integration_install
     */
    info!("开始生成本地 hook 包装器并安装集成");

    // 1.1 先生成本地 hook 包装器
    write_hook_wrappers(app)?;

    // 1.2 遍历注册表安装每个集成
    let mut actions = Vec::new();
    for integ in registry::registered_integrations() {
        let source = integ.source();
        let action = collect_install_result(store, source, integ.install(app, store))?;
        actions.push(action);
    }

    info!("完成本地 hook 包装器生成与集成安装");
    Ok(actions)
}

pub fn uninstall_all(app: &AppContext, store: &Store) -> Result<Vec<IntegrationAction>> {
    registry::registered_integrations()
        .iter()
        .map(|integ| integ.uninstall(app, store))
        .collect()
}

pub fn write_hook_wrappers(app: &AppContext) -> Result<()> {
    let cmd_body = format!(
        "@echo off\r\n\"{}\" hook-run %*\r\n",
        app.current_exe.to_string_lossy().replace('"', "\"\"")
    );
    // POSIX single quotes: `"..."` would still expand $VAR / $(cmd) / backticks
    // if the executable path contained them.
    let sh_body = format!(
        "#!/usr/bin/env sh\n{} hook-run \"$@\"\n",
        hook_target::quote_posix(&app.current_exe.to_string_lossy())
    );

    fs::create_dir_all(&app.paths.bin_dir)?;
    write_file_atomic(&app.paths.hook_cmd_path, cmd_body)?;
    write_file_atomic(&app.paths.hook_sh_path, sh_body)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&app.paths.hook_sh_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&app.paths.hook_sh_path, perms)?;
    }

    Ok(())
}

/// Copies `original` into `backups_dir` under a name that is guaranteed not to
/// collide with an existing backup.
///
/// The previous implementation used a second-granularity timestamp plus
/// `fs::copy`, which silently overwrites. Two installs in the same second
/// destroyed the genuinely-original backup that uninstall depends on. This
/// version appends nanoseconds and retries with an increasing counter using
/// `create_new(true)`, so an existing backup is never clobbered.
pub fn backup_file(original: &Path, backups_dir: &Path, stem: &str) -> Result<PathBuf> {
    fs::create_dir_all(backups_dir)?;
    let timestamp = now_utc().replace(':', "-");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);

    for attempt in 0..1_000u32 {
        let candidate = if attempt == 0 {
            backups_dir.join(format!("{stem}.{timestamp}.{nanos:09}.bak"))
        } else {
            backups_dir.join(format!("{stem}.{timestamp}.{nanos:09}.{attempt}.bak"))
        };
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut dest) => {
                let mut src = fs::File::open(original)?;
                std::io::copy(&mut src, &mut dest)?;
                dest.sync_all()?;
                return Ok(candidate);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    anyhow::bail!("无法为 {stem} 创建唯一备份文件名（已尝试 1000 次）")
}

pub fn record_probe(store: &Store, probe: &IntegrationProbe) -> Result<()> {
    Ok(store.integration_state().record_integration_state(
        probe.source,
        "probe",
        &probe.status,
        probe.config_path.as_deref().map(Path::new),
        None,
        Some(&json!({ "detail": probe.detail })),
    )?)
}

pub fn record_action(
    store: &Store,
    source: SourceKind,
    install_type: &str,
    status: &str,
    detail: &str,
    config_path: Option<&Path>,
    backup_path: Option<&Path>,
) -> Result<()> {
    Ok(store.integration_state().record_integration_state(
        source,
        install_type,
        status,
        config_path,
        backup_path,
        Some(&json!({ "detail": detail })),
    )?)
}

fn collect_install_result(
    store: &Store,
    source: SourceKind,
    result: Result<IntegrationAction>,
) -> Result<IntegrationAction> {
    match result {
        Ok(action) => Ok(action),
        Err(err) => {
            let detail = format!("{err:#}");
            record_action(store, source, "init", "error", &detail, None, None)?;
            Ok(IntegrationAction {
                source,
                status: "error".to_string(),
                detail,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// REL-004: the old implementation used a second-granularity timestamp and
    /// `fs::copy`, which overwrites. Two installs within the same second
    /// destroyed the genuinely-original backup that uninstall restores from.
    #[test]
    fn repeated_backups_in_same_second_do_not_overwrite() {
        let temp = TempDir::new().expect("temp dir");
        let original = temp.path().join("settings.json");
        let backups = temp.path().join("backups");

        fs::write(&original, br#"{"first":true}"#).expect("write original");
        let first = backup_file(&original, &backups, "claude-settings").expect("first backup");

        fs::write(&original, br#"{"second":true}"#).expect("overwrite original");
        let second = backup_file(&original, &backups, "claude-settings").expect("second backup");

        assert_ne!(first, second, "backup paths must be unique");
        assert_eq!(
            fs::read(&first).expect("read first"),
            br#"{"first":true}"#.to_vec(),
            "the original backup content must survive a later backup"
        );
        assert_eq!(
            fs::read(&second).expect("read second"),
            br#"{"second":true}"#.to_vec()
        );
    }

    /// REL-003: a plain `fs::write` truncates in place, so a crash mid-write
    /// leaves a corrupt config. The atomic path must leave either old or new.
    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_file() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("config.json");
        fs::write(&target, b"old").expect("seed target");

        write_file_atomic(&target, b"new content").expect("atomic write");

        assert_eq!(fs::read(&target).expect("read target"), b"new content");
        let leftovers = fs::read_dir(temp.path())
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("llmusage-tmp"))
            .count();
        assert_eq!(leftovers, 0, "no temp file may be left behind");
    }

    #[test]
    fn atomic_write_creates_missing_parent_directories() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp
            .path()
            .join("nested")
            .join("deeper")
            .join("config.json");
        write_file_atomic(&target, b"payload").expect("atomic write");
        assert_eq!(fs::read(&target).expect("read target"), b"payload");
    }
}
