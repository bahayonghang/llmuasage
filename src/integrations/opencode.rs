use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{app::AppContext, models::SourceKind, store::Store};

use super::{
    IntegrationAction, backup_file, record_action, recover_and_cleanup_residue,
    remove_file_atomic_and_record,
};

const PLUGIN_MARKER: &str = "LLMUSAGE_LOCAL_PLUGIN";
const PLUGIN_NAME: &str = "llmusage-tracker.js";
const OPENCODE_DB_NAME: &str = "opencode.db";

pub fn cleanup(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let plugin_path = resolve_plugin_path(app);
    let residue_removed = recover_and_cleanup_residue(&plugin_path)?;
    if !plugin_path.exists() {
        if residue_removed {
            record_action(
                store,
                SourceKind::Opencode,
                "legacy-cleanup",
                "restored",
                "recovered legacy OpenCode atomic-write residue",
                Some(&plugin_path),
                None,
            )?;
        }
        return Ok(IntegrationAction {
            source: SourceKind::Opencode,
            status: if residue_removed {
                "restored"
            } else {
                "skipped"
            }
            .to_string(),
            detail: if residue_removed {
                "recovered legacy OpenCode atomic-write residue"
            } else {
                "no legacy OpenCode plugin found"
            }
            .to_string(),
        });
    }

    let content = fs::read_to_string(&plugin_path)?;
    if !content.contains(PLUGIN_MARKER) {
        if residue_removed {
            record_action(
                store,
                SourceKind::Opencode,
                "legacy-cleanup",
                "restored",
                "recovered residue; preserved unowned OpenCode plugin",
                Some(&plugin_path),
                None,
            )?;
        }
        return Ok(IntegrationAction {
            source: SourceKind::Opencode,
            status: if residue_removed {
                "restored"
            } else {
                "skipped"
            }
            .to_string(),
            detail: if residue_removed {
                "recovered residue; preserved unowned OpenCode plugin"
            } else {
                "preserved unowned OpenCode plugin"
            }
            .to_string(),
        });
    }

    let backup_path = backup_file(
        &plugin_path,
        &app.paths.backups_dir,
        "opencode-plugin-legacy-cleanup",
    )?;
    let record = || {
        record_action(
            store,
            SourceKind::Opencode,
            "legacy-cleanup",
            "restored",
            "removed legacy llmusage OpenCode plugin",
            Some(&plugin_path),
            Some(&backup_path),
        )
    };
    remove_file_atomic_and_record(&plugin_path, record)?;

    Ok(IntegrationAction {
        source: SourceKind::Opencode,
        status: "restored".to_string(),
        detail: "removed legacy llmusage OpenCode plugin".to_string(),
    })
}

fn official_storage_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(".local").join("share").join("opencode")
}

fn legacy_storage_dir(home_dir: &Path) -> PathBuf {
    legacy_data_local_dir(home_dir).join("opencode")
}

fn legacy_data_local_dir(home_dir: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir.join("Library").join("Application Support")
    }

    #[cfg(target_os = "windows")]
    {
        home_dir.join("AppData").join("Local")
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home_dir.join(".local").join("share")
    }
}

pub(crate) fn resolve_default_storage_dir(home_dir: &Path) -> PathBuf {
    let official = official_storage_dir(home_dir);
    if official.exists() {
        return official;
    }

    let legacy = legacy_storage_dir(home_dir);
    if legacy.exists() {
        return legacy;
    }

    official
}

pub(crate) fn resolve_db_path() -> PathBuf {
    discover_db_paths().into_iter().next().unwrap_or_else(|| {
        let home_dir = crate::util::resolve_home_dir();
        resolve_default_storage_dir(&home_dir).join(OPENCODE_DB_NAME)
    })
}

pub(crate) fn discover_db_paths() -> Vec<PathBuf> {
    let home_dir = crate::util::resolve_home_dir();
    discover_db_paths_with_home(&home_dir)
}

pub(crate) fn discover_db_paths_with_home(home_dir: &Path) -> Vec<PathBuf> {
    if let Some(explicit) = env_path("OPENCODE_DB") {
        return vec![explicit];
    }

    let roots = if let Some(opencode_home) = env_path("OPENCODE_HOME") {
        vec![opencode_home]
    } else {
        candidate_storage_dirs(home_dir)
    };

    let mut discovered = Vec::new();
    let mut seen = BTreeSet::new();
    for root in &roots {
        for candidate in discover_opencode_dbs(root) {
            let key = candidate
                .canonicalize()
                .unwrap_or_else(|_| candidate.clone());
            if seen.insert(key) {
                discovered.push(candidate);
            }
        }
    }

    sort_opencode_dbs(&mut discovered);
    if discovered.is_empty() {
        let fallback_root = roots
            .first()
            .cloned()
            .unwrap_or_else(|| resolve_default_storage_dir(home_dir));
        discovered.push(fallback_root.join(OPENCODE_DB_NAME));
    }
    discovered
}

fn candidate_storage_dirs(home_dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![official_storage_dir(home_dir)];
    let legacy = legacy_storage_dir(home_dir);
    if legacy != roots[0] {
        roots.push(legacy);
    }
    roots
}

fn discover_opencode_dbs(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(data_dir) else {
        return Vec::new();
    };
    let mut dbs = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(is_opencode_db_filename)
        })
        .collect::<Vec<_>>();
    sort_opencode_dbs(&mut dbs);
    dbs
}

fn sort_opencode_dbs(paths: &mut [PathBuf]) {
    paths.sort_by(|left, right| {
        let left_name = left
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let right_name = right
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        opencode_db_rank(left_name)
            .cmp(&opencode_db_rank(right_name))
            .then_with(|| left.cmp(right))
    });
}

fn opencode_db_rank(name: &str) -> u8 {
    if name == OPENCODE_DB_NAME { 0 } else { 1 }
}

fn is_opencode_db_filename(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".db") else {
        return false;
    };
    if stem == "opencode" {
        return true;
    }
    let Some(channel) = stem.strip_prefix("opencode-") else {
        return false;
    };
    !channel.is_empty()
        && channel
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve_plugin_path(_app: &AppContext) -> PathBuf {
    let config_dir = std::env::var("OPENCODE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode")
        });
    config_dir.join("plugin").join(PLUGIN_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_storage_prefers_official_home_data_dir() {
        let temp = tempfile::tempdir().expect("temp dir");
        let official = temp.path().join(".local").join("share").join("opencode");
        let legacy = legacy_storage_dir(temp.path());
        std::fs::create_dir_all(&official).expect("official dir");
        std::fs::create_dir_all(&legacy).expect("legacy dir");

        assert_eq!(resolve_default_storage_dir(temp.path()), official);
    }

    #[test]
    fn default_storage_falls_back_to_official_even_when_absent() {
        let temp = tempfile::tempdir().expect("temp dir");

        assert_eq!(
            resolve_default_storage_dir(temp.path()),
            temp.path().join(".local").join("share").join("opencode"),
        );
    }

    #[test]
    fn opencode_db_discovery_prefers_default_then_channel_dbs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let data_dir = temp.path().join(".local").join("share").join("opencode");
        std::fs::create_dir_all(&data_dir).expect("data dir");
        std::fs::write(data_dir.join("opencode-stable.db"), "").expect("stable db");
        std::fs::write(data_dir.join("opencode.db"), "").expect("default db");
        std::fs::write(data_dir.join("opencode-nightly.db"), "").expect("nightly db");
        std::fs::write(data_dir.join("opencode.db-wal"), "").expect("wal sidecar");
        std::fs::write(data_dir.join("other.db"), "").expect("unrelated db");

        let paths = discover_db_paths_with_home(temp.path());
        let names = paths
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "opencode.db".to_string(),
                "opencode-nightly.db".to_string(),
                "opencode-stable.db".to_string()
            ]
        );
    }

    #[test]
    fn opencode_db_discovery_falls_back_to_legacy_storage() {
        let temp = tempfile::tempdir().expect("temp dir");
        let legacy = legacy_storage_dir(temp.path());
        std::fs::create_dir_all(&legacy).expect("legacy dir");
        std::fs::write(legacy.join("opencode-stable.db"), "").expect("legacy db");

        assert_eq!(
            discover_db_paths_with_home(temp.path()),
            vec![legacy.join("opencode-stable.db")]
        );
    }

    #[test]
    fn explicit_opencode_db_env_wins_over_discovery() {
        let temp = tempfile::tempdir().expect("temp dir");
        let explicit = temp.path().join("opencode-stable.db");
        let _guard = EnvGuard::set("OPENCODE_DB", &explicit);

        assert_eq!(discover_db_paths_with_home(temp.path()), vec![explicit]);
    }

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var(self.key, previous);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
}
