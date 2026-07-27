use std::{fs, path::PathBuf};

use anyhow::{Result, bail};
use toml_edit::{DocumentMut, Item, Value, value};

use crate::{app::AppContext, models::SourceKind, store::Store, util::resolve_home_dir};

use super::{
    IntegrationAction, backup_file, record_action, recover_and_cleanup_residue,
    write_file_atomic_and_record,
};

pub fn cleanup(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let config_path = resolve_codex_config();
    let backup_value_path = app.paths.backups_dir.join("codex_notify_original.json");
    let residue_removed = recover_and_cleanup_residue(&config_path)?;
    if !config_path.is_file() {
        if backup_value_path.exists() {
            bail!(
                "cannot restore {} because {} is missing",
                backup_value_path.display(),
                config_path.display()
            );
        }
        return finish_residue_only(store, &config_path, residue_removed);
    }

    let raw = fs::read_to_string(&config_path)?;
    let mut doc = raw.parse::<DocumentMut>()?;
    let current = read_notify(&doc);
    let restore_marker = backup_value_path.is_file();
    let mut changed = false;

    if restore_marker {
        let backup_json: serde_json::Value =
            serde_json::from_slice(&fs::read(&backup_value_path)?)?;
        match backup_json
            .get("notify")
            .and_then(serde_json::Value::as_array)
        {
            Some(values) => {
                let restored = values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(Value::from)
                    .collect::<toml_edit::Array>();
                doc["notify"] = value(restored);
            }
            None => {
                doc.remove("notify");
            }
        }
        changed = doc.to_string() != raw;
    } else if current
        .as_ref()
        .is_some_and(|args| is_llmusage_notify(args))
    {
        doc.remove("notify");
        changed = true;
    }

    if !changed && !restore_marker {
        return finish_residue_only(store, &config_path, residue_removed);
    }

    let backup_path = changed
        .then(|| {
            backup_file(
                &config_path,
                &app.paths.backups_dir,
                "codex-config-legacy-cleanup",
            )
        })
        .transpose()?;
    if changed {
        write_file_atomic_and_record(&config_path, doc.to_string(), || {
            record_action(
                store,
                SourceKind::Codex,
                "legacy-cleanup",
                "restored",
                "restored Codex notify after legacy llmusage hook removal",
                Some(&config_path),
                backup_path.as_deref(),
            )
        })?;
    } else {
        record_action(
            store,
            SourceKind::Codex,
            "legacy-cleanup",
            "restored",
            "consumed an already-restored Codex notify marker",
            Some(&config_path),
            None,
        )?;
    }

    if restore_marker {
        fs::remove_file(&backup_value_path)?;
    }
    Ok(IntegrationAction {
        source: SourceKind::Codex,
        status: "restored".to_string(),
        detail: "legacy Codex notify cleanup completed".to_string(),
    })
}

fn finish_residue_only(
    store: &Store,
    config_path: &std::path::Path,
    residue_removed: bool,
) -> Result<IntegrationAction> {
    if residue_removed {
        record_action(
            store,
            SourceKind::Codex,
            "legacy-cleanup",
            "restored",
            "recovered legacy Codex atomic-write residue",
            Some(config_path),
            None,
        )?;
        Ok(IntegrationAction {
            source: SourceKind::Codex,
            status: "restored".to_string(),
            detail: "recovered legacy Codex atomic-write residue".to_string(),
        })
    } else {
        Ok(IntegrationAction {
            source: SourceKind::Codex,
            status: "skipped".to_string(),
            detail: "no legacy Codex notify found".to_string(),
        })
    }
}

fn resolve_codex_config() -> PathBuf {
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| resolve_home_dir().join(".codex"))
        .join("config.toml")
}

fn read_notify(doc: &DocumentMut) -> Option<Vec<String>> {
    doc.get("notify").and_then(Item::as_array).map(|array| {
        array
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect()
    })
}

fn is_llmusage_notify(args: &[String]) -> bool {
    args.iter().any(|arg| arg.contains("llmusage-hook"))
}

#[cfg(test)]
mod tests {
    use super::is_llmusage_notify;

    #[test]
    fn notify_matching_uses_the_stable_wrapper_name() {
        assert!(is_llmusage_notify(&["C:/x/llmusage-hook.cmd".into()]));
        assert!(!is_llmusage_notify(&[
            "C:/work/llmusage/codex-computer-use.exe".into()
        ]));
    }
}
