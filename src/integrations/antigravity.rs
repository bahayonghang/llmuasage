use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::{app::AppContext, models::SourceKind, store::Store, util::resolve_home_dir};

use super::{
    IntegrationAction, backup_file, record_action, recover_and_cleanup_residue,
    write_file_atomic_and_record,
};

const ANTIGRAVITY_HOOK_EVENT: &str = "Stop";
const LEGACY_GEMINI_HOOK_EVENT: &str = "SessionEnd";

pub fn cleanup(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let antigravity_path = resolve_antigravity_hooks();
    let legacy_path = resolve_legacy_gemini_settings();
    let mut changed_any = false;

    changed_any |= cleanup_antigravity_file(app, store, &antigravity_path)?;
    changed_any |= cleanup_legacy_gemini_file(app, store, &legacy_path)?;

    Ok(IntegrationAction {
        source: SourceKind::Antigravity,
        status: if changed_any { "restored" } else { "skipped" }.to_string(),
        detail: if changed_any {
            "removed legacy llmusage Antigravity/Gemini hooks"
        } else {
            "no legacy Antigravity/Gemini hooks found"
        }
        .to_string(),
    })
}

fn cleanup_antigravity_file(app: &AppContext, store: &Store, path: &Path) -> Result<bool> {
    let residue_removed = recover_and_cleanup_residue(path)?;
    let mut changed = false;
    if path.is_file() {
        let mut settings = read_settings(path)?;
        changed = remove_direct_llmusage_commands(&mut settings, ANTIGRAVITY_HOOK_EVENT)?;
        if changed {
            let backup_path = backup_file(
                path,
                &app.paths.backups_dir,
                "antigravity-hooks-legacy-cleanup",
            )?;
            write_file_atomic_and_record(path, serde_json::to_vec_pretty(&settings)?, || {
                record_action(
                    store,
                    SourceKind::Antigravity,
                    "legacy-cleanup",
                    "restored",
                    "removed legacy llmusage Antigravity hooks",
                    Some(path),
                    Some(&backup_path),
                )
            })?;
        }
    }
    if residue_removed && !changed {
        record_action(
            store,
            SourceKind::Antigravity,
            "legacy-cleanup",
            "restored",
            "recovered legacy Antigravity atomic-write residue",
            Some(path),
            None,
        )?;
    }
    Ok(changed || residue_removed)
}

fn cleanup_legacy_gemini_file(app: &AppContext, store: &Store, path: &Path) -> Result<bool> {
    let residue_removed = recover_and_cleanup_residue(path)?;
    let mut changed = false;
    if path.is_file() {
        let mut settings = read_settings(path)?;
        changed = remove_nested_llmusage_commands(&mut settings, LEGACY_GEMINI_HOOK_EVENT)?;
        if changed {
            let backup_path = backup_file(
                path,
                &app.paths.backups_dir,
                "legacy-gemini-settings-cleanup",
            )?;
            write_file_atomic_and_record(path, serde_json::to_vec_pretty(&settings)?, || {
                record_action(
                    store,
                    SourceKind::Antigravity,
                    "legacy-cleanup",
                    "restored",
                    "removed legacy llmusage Gemini hooks",
                    Some(path),
                    Some(&backup_path),
                )
            })?;
        }
    }
    if residue_removed && !changed {
        record_action(
            store,
            SourceKind::Antigravity,
            "legacy-cleanup",
            "restored",
            "recovered legacy Gemini atomic-write residue",
            Some(path),
            None,
        )?;
    }
    Ok(changed || residue_removed)
}

fn resolve_antigravity_hooks() -> PathBuf {
    resolve_home_dir()
        .join(".gemini")
        .join("config")
        .join("hooks.json")
}

fn resolve_legacy_gemini_settings() -> PathBuf {
    resolve_home_dir().join(".gemini").join("settings.json")
}

fn read_settings(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn remove_direct_llmusage_commands(settings: &mut Value, event: &str) -> Result<bool> {
    let root = root_object_mut(settings, "Antigravity hooks.json")?;
    let Some(entries_value) = root.get_mut(event) else {
        return Ok(false);
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Antigravity hooks.{event} must be an array"))?;
    let before = entries.len();
    entries.retain(|entry| {
        !entry
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(is_llmusage_hook_command)
    });
    Ok(entries.len() != before)
}

fn remove_nested_llmusage_commands(settings: &mut Value, event: &str) -> Result<bool> {
    let root = root_object_mut(settings, "Gemini settings.json")?;
    let Some(hooks_value) = root.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks_value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Gemini settings.json hooks field must be an object"))?;
    let Some(entries_value) = hooks.get_mut(event) else {
        return Ok(false);
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Gemini hooks.{event} must be an array"))?;
    let mut changed = false;
    for entry in entries.iter_mut() {
        let Some(commands) = entry.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        let before = commands.len();
        commands.retain(|hook| {
            !hook
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(is_llmusage_hook_command)
        });
        changed |= commands.len() != before;
    }
    entries.retain(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|commands| !commands.is_empty())
    });
    Ok(changed)
}

fn is_llmusage_hook_command(command: &str) -> bool {
    command.contains("llmusage-hook")
}

fn root_object_mut<'a>(
    settings: &'a mut Value,
    label: &str,
) -> Result<&'a mut serde_json::Map<String, Value>> {
    settings
        .as_object_mut()
        .ok_or_else(|| anyhow!("{label} top level must be an object"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn cleanup_removes_current_and_legacy_commands_but_keeps_user_hooks() -> Result<()> {
        let mut hooks = json!({
            "Stop": [
                { "type": "command", "command": "notify-user" },
                { "type": "command", "command": "llmusage-hook --source antigravity" },
                { "type": "command", "command": "llmusage-hook --source gemini" }
            ]
        });
        assert!(remove_direct_llmusage_commands(&mut hooks, "Stop")?);
        assert_eq!(
            hooks["Stop"],
            json!([{ "type": "command", "command": "notify-user" }])
        );
        Ok(())
    }

    #[test]
    fn legacy_cleanup_preserves_sibling_user_command() -> Result<()> {
        let mut settings = json!({
            "hooks": {
                "SessionEnd": [{ "hooks": [
                    { "type": "command", "command": "notify-user" },
                    { "type": "command", "command": "llmusage-hook --source gemini" }
                ] }]
            }
        });
        assert!(remove_nested_llmusage_commands(
            &mut settings,
            "SessionEnd"
        )?);
        assert_eq!(
            settings["hooks"]["SessionEnd"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        Ok(())
    }
}
