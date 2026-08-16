use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::json;

use crate::{app::AppContext, models::SourceKind, store::Store, util::now_utc};

pub mod antigravity;
mod atomic;
pub mod claude;
pub mod codex;
pub mod opencode;
pub mod zcode;

pub use atomic::{
    recover_and_cleanup_residue, remove_file_atomic_and_record, write_file_atomic,
    write_file_atomic_and_record,
};

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationAction {
    pub source: SourceKind,
    pub status: String,
    pub detail: String,
}

type CleanupFn = fn(&AppContext, &Store) -> Result<IntegrationAction>;
const LEGACY_HOOK_WRAPPERS_AUDIT_KEY: &str = "legacy_hook_wrappers";

/// Removes hook/plugin artifacts installed by older llmusage releases.
pub fn cleanup_all(app: &AppContext, store: &Store) -> Result<Vec<IntegrationAction>> {
    let cleanups: [(SourceKind, CleanupFn); 4] = [
        (SourceKind::Codex, codex::cleanup),
        (SourceKind::Claude, claude::cleanup),
        (SourceKind::Opencode, opencode::cleanup),
        (SourceKind::Antigravity, antigravity::cleanup),
    ];
    let mut actions = Vec::with_capacity(cleanups.len());
    let mut failures = Vec::new();

    for (source, cleanup) in cleanups {
        match cleanup(app, store) {
            Ok(action) => actions.push(action),
            Err(error) => {
                let detail = format!("{error:#}");
                if let Err(audit_error) = record_action(
                    store,
                    source,
                    "legacy-cleanup",
                    "error",
                    &detail,
                    None,
                    None,
                ) {
                    failures.push(format!(
                        "{source}: {detail}; cleanup failure audit also failed: {audit_error:#}"
                    ));
                    continue;
                }
                failures.push(format!("{source}: {detail}"));
            }
        }
    }

    if let Err(error) = cleanup_hook_wrappers(app, store) {
        failures.push(format!("legacy hook wrappers: {error:#}"));
    }
    if !failures.is_empty() {
        bail!("legacy hook cleanup failed: {}", failures.join("; "));
    }
    Ok(actions)
}

fn cleanup_hook_wrappers(app: &AppContext, store: &Store) -> Result<()> {
    let mut removed = Vec::new();
    let mut failures = Vec::new();
    for path in [&app.paths.hook_cmd_path, &app.paths.hook_sh_path] {
        if path.is_file() {
            let path_display = path.display().to_string();
            let detail = format!("removed legacy hook wrapper: {path_display}");
            match remove_file_atomic_and_record(path, || {
                record_action_for_key(
                    store,
                    LEGACY_HOOK_WRAPPERS_AUDIT_KEY,
                    "legacy-cleanup",
                    "restored",
                    &detail,
                    Some(path),
                    None,
                )
            }) {
                Ok(()) => removed.push(path_display),
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
        }
    }
    if app.paths.bin_dir.is_dir() {
        match fs::read_dir(&app.paths.bin_dir) {
            Ok(mut entries) => {
                if entries.next().is_none()
                    && let Err(error) = fs::remove_dir(&app.paths.bin_dir)
                {
                    failures.push(format!("{}: {error}", app.paths.bin_dir.display()));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", app.paths.bin_dir.display())),
        }
    }

    if failures.is_empty() {
        return Ok(());
    }

    let mut detail = if removed.is_empty() {
        "no legacy hook wrappers removed".to_string()
    } else {
        format!("removed legacy hook wrapper(s): {}", removed.join(", "))
    };
    if !failures.is_empty() {
        detail.push_str(&format!("; failures: {}", failures.join("; ")));
    }
    record_action_for_key(
        store,
        LEGACY_HOOK_WRAPPERS_AUDIT_KEY,
        "legacy-cleanup",
        "error",
        &detail,
        Some(&app.paths.bin_dir),
        None,
    )?;
    bail!(detail)
}

/// Copies `original` into `backups_dir` without overwriting an older backup.
pub fn backup_file(original: &Path, backups_dir: &Path, stem: &str) -> Result<PathBuf> {
    fs::create_dir_all(backups_dir)?;
    let timestamp = now_utc().replace(':', "-");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
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
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("unable to create a unique backup name for {stem} after 1000 attempts")
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
    record_action_for_key(
        store,
        source.as_str(),
        install_type,
        status,
        detail,
        config_path,
        backup_path,
    )
}

fn record_action_for_key(
    store: &Store,
    audit_key: &str,
    install_type: &str,
    status: &str,
    detail: &str,
    config_path: Option<&Path>,
    backup_path: Option<&Path>,
) -> Result<()> {
    Ok(store.integration_state().record_integration_state_for_key(
        audit_key,
        install_type,
        status,
        config_path,
        backup_path,
        Some(&json!({ "detail": detail })),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn repeated_backups_in_same_second_do_not_overwrite() {
        let temp = TempDir::new().expect("temp dir");
        let original = temp.path().join("settings.json");
        let backups = temp.path().join("backups");

        fs::write(&original, br#"{"first":true}"#).expect("write original");
        let first = backup_file(&original, &backups, "claude-settings").expect("first backup");
        fs::write(&original, br#"{"second":true}"#).expect("overwrite original");
        let second = backup_file(&original, &backups, "claude-settings").expect("second backup");

        assert_ne!(first, second);
        assert_eq!(fs::read(first).expect("read first"), br#"{"first":true}"#);
        assert_eq!(
            fs::read(second).expect("read second"),
            br#"{"second":true}"#
        );
    }
}
