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

pub fn cleanup(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let settings_path = resolve_claude_settings();
    let residue_removed = recover_and_cleanup_residue(&settings_path)?;
    if !settings_path.is_file() {
        return finish_residue_only(store, &settings_path, residue_removed);
    }

    let mut settings = read_settings(&settings_path)?;
    let changed = remove_llmusage_event_commands(&mut settings, "Stop")?
        | remove_llmusage_event_commands(&mut settings, "SessionEnd")?;
    if !changed {
        return finish_residue_only(store, &settings_path, residue_removed);
    }

    let backup_path = backup_file(
        &settings_path,
        &app.paths.backups_dir,
        "claude-settings-legacy-cleanup",
    )?;
    write_file_atomic_and_record(
        &settings_path,
        serde_json::to_vec_pretty(&settings)?,
        || {
            record_action(
                store,
                SourceKind::Claude,
                "legacy-cleanup",
                "restored",
                "removed legacy llmusage Claude hooks",
                Some(&settings_path),
                Some(&backup_path),
            )
        },
    )?;

    Ok(restored("removed legacy llmusage Claude hooks"))
}

fn finish_residue_only(
    store: &Store,
    settings_path: &Path,
    residue_removed: bool,
) -> Result<IntegrationAction> {
    if residue_removed {
        record_action(
            store,
            SourceKind::Claude,
            "legacy-cleanup",
            "restored",
            "recovered legacy Claude atomic-write residue",
            Some(settings_path),
            None,
        )?;
        Ok(restored("recovered legacy Claude atomic-write residue"))
    } else {
        Ok(IntegrationAction {
            source: SourceKind::Claude,
            status: "skipped".to_string(),
            detail: "no legacy Claude hooks found".to_string(),
        })
    }
}

fn restored(detail: &str) -> IntegrationAction {
    IntegrationAction {
        source: SourceKind::Claude,
        status: "restored".to_string(),
        detail: detail.to_string(),
    }
}

fn resolve_claude_settings() -> PathBuf {
    resolve_home_dir().join(".claude").join("settings.json")
}

fn read_settings(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn remove_llmusage_event_commands(settings: &mut Value, event: &str) -> Result<bool> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| anyhow!("Claude settings.json top level must be an object"))?;
    let Some(hooks_value) = root.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks_value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Claude settings.json hooks field must be an object"))?;
    let Some(entries_value) = hooks.get_mut(event) else {
        return Ok(false);
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Claude hooks.{event} must be an array"))?;
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn cleanup_removes_historical_quoting_variants_and_preserves_sibling_hooks() -> Result<()> {
        let user = json!({ "type": "command", "command": "notify-user" });
        let mut settings = json!({
            "hooks": {
                "Stop": [
                    { "hooks": [
                        user.clone(),
                        { "type": "command", "command": "cmd /c \"C:\\\\x\\\\llmusage-hook.cmd --source claude\"" }
                    ] },
                    { "hooks": [
                        { "type": "command", "command": "cmd /c \"\"C:\\\\x\\\\llmusage-hook.cmd\" --source claude\"" }
                    ] }
                ]
            },
            "keep": { "nested": true }
        });

        assert!(remove_llmusage_event_commands(&mut settings, "Stop")?);
        assert_eq!(settings["hooks"]["Stop"], json!([{ "hooks": [user] }]));
        assert_eq!(settings["keep"], json!({ "nested": true }));
        Ok(())
    }
}
