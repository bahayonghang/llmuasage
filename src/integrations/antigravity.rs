use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use serde_json::{Map, Value, json};

use crate::{app::AppContext, models::SourceKind, store::Store, util::resolve_home_dir};

use super::{
    HookTarget, Integration, IntegrationAction, IntegrationProbe, backup_file, record_action,
    record_probe,
};

const ANTIGRAVITY_HOOK_EVENT: &str = "Stop";
const LEGACY_GEMINI_HOOK_EVENT: &str = "SessionEnd";

/// ZST handle implementing [`Integration`] for Google Antigravity hooks.
///
/// Antigravity is the public llmusage source id. The integration only installs
/// `~/.gemini/config/hooks.json::Stop`; legacy Gemini CLI `settings.json` hooks
/// are cleaned up best-effort when they contain llmusage-owned `--source gemini`
/// commands.
pub struct AntigravityIntegration;

impl Integration for AntigravityIntegration {
    fn source(&self) -> SourceKind {
        SourceKind::Antigravity
    }

    fn probe(&self, app: &AppContext) -> Result<IntegrationProbe> {
        probe(app)
    }

    fn install(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction> {
        install(app, store)
    }

    fn uninstall(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction> {
        uninstall(app, store)
    }
}

pub fn probe(app: &AppContext) -> Result<IntegrationProbe> {
    let target = antigravity_target();
    let hook = HookTarget::current(app);

    let (status, detail, config_path) = if target.path.is_file() {
        let settings = read_settings(&target.path)?;
        if antigravity_event_has_command(
            &settings,
            ANTIGRAVITY_HOOK_EVENT,
            &hook.shell_command(SourceKind::Antigravity, ANTIGRAVITY_HOOK_EVENT),
        ) {
            (
                "ready",
                "Antigravity Stop hook is installed".to_string(),
                Some(target.path.to_string_lossy().to_string()),
            )
        } else {
            (
                "drifted",
                "Antigravity Stop hook needs install or update".to_string(),
                Some(target.path.to_string_lossy().to_string()),
            )
        }
    } else if target.installable {
        (
            "drifted",
            "Antigravity hooks.json is missing and can be created".to_string(),
            Some(target.path.to_string_lossy().to_string()),
        )
    } else {
        (
            "missing",
            "Antigravity CLI config directory was not found".to_string(),
            None,
        )
    };

    Ok(IntegrationProbe {
        source: SourceKind::Antigravity,
        status: status.to_string(),
        detail,
        config_path,
    })
}

pub fn install(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let target = antigravity_target();
    let hook = HookTarget::current(app);
    let cleanup = cleanup_legacy_llmusage_gemini_hooks(app, &hook)?;

    if !target.installable {
        let probe = probe(app)?;
        record_probe(store, &probe)?;
        return Ok(IntegrationAction {
            source: SourceKind::Antigravity,
            status: "skipped".to_string(),
            detail: format!(
                "Antigravity CLI config directory was not found; skipped Stop hook install{cleanup}"
            ),
        });
    }

    let mut settings = if target.path.is_file() {
        read_settings(&target.path)?
    } else {
        json!({})
    };
    let backup_path = if target.path.is_file() {
        Some(backup_file(
            &target.path,
            &app.paths.backups_dir,
            "antigravity-hooks",
        )?)
    } else {
        None
    };
    remove_legacy_antigravity_command(&mut settings)?;
    ensure_antigravity_event_command(
        &mut settings,
        ANTIGRAVITY_HOOK_EVENT,
        &hook.shell_command(SourceKind::Antigravity, ANTIGRAVITY_HOOK_EVENT),
    )?;

    if let Some(parent) = target.path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target.path, serde_json::to_vec_pretty(&settings)?)?;

    let detail = format!("Antigravity Stop hook installed{cleanup}");
    record_action(
        store,
        SourceKind::Antigravity,
        "init",
        "ready",
        &detail,
        Some(&target.path),
        backup_path.as_deref(),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Antigravity,
        status: "ready".to_string(),
        detail,
    })
}

pub fn uninstall(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let target = antigravity_target();
    let hook = HookTarget::current(app);
    let cleanup = cleanup_legacy_llmusage_gemini_hooks(app, &hook)?;

    if !target.path.is_file() {
        return Ok(IntegrationAction {
            source: SourceKind::Antigravity,
            status: "skipped".to_string(),
            detail: format!("Antigravity hook config does not exist{cleanup}"),
        });
    }

    let mut settings = read_settings(&target.path)?;
    let backup_path = backup_file(
        &target.path,
        &app.paths.backups_dir,
        "antigravity-hooks-restore",
    )?;
    remove_antigravity_event_command(
        &mut settings,
        ANTIGRAVITY_HOOK_EVENT,
        &hook.shell_command(SourceKind::Antigravity, ANTIGRAVITY_HOOK_EVENT),
    )?;
    remove_legacy_antigravity_command(&mut settings)?;
    fs::write(&target.path, serde_json::to_vec_pretty(&settings)?)?;

    let detail = format!("Antigravity Stop hook restored{cleanup}");
    record_action(
        store,
        SourceKind::Antigravity,
        "uninstall",
        "restored",
        &detail,
        Some(&target.path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Antigravity,
        status: "restored".to_string(),
        detail,
    })
}

struct HookConfigTarget {
    path: PathBuf,
    installable: bool,
}

fn antigravity_target() -> HookConfigTarget {
    let home_dir = resolve_home_dir();
    let path = resolve_antigravity_hooks();
    HookConfigTarget {
        installable: path.is_file()
            || home_dir.join(".gemini").join("antigravity-cli").exists()
            || home_dir.join(".gemini").join("antigravity").exists()
            || path.parent().is_some_and(Path::exists),
        path,
    }
}

fn resolve_antigravity_hooks() -> PathBuf {
    let home_dir = resolve_home_dir();
    home_dir.join(".gemini").join("config").join("hooks.json")
}

fn resolve_legacy_gemini_settings() -> PathBuf {
    let home_dir = resolve_home_dir();
    home_dir.join(".gemini").join("settings.json")
}

fn read_settings(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn cleanup_legacy_llmusage_gemini_hooks(app: &AppContext, hook: &HookTarget) -> Result<String> {
    let mut cleaned = Vec::new();

    let antigravity_path = resolve_antigravity_hooks();
    if antigravity_path.is_file() {
        let mut settings = read_settings(&antigravity_path)?;
        if remove_legacy_antigravity_command(&mut settings)? {
            backup_file(
                &antigravity_path,
                &app.paths.backups_dir,
                "antigravity-hooks-legacy-cleanup",
            )?;
            fs::write(&antigravity_path, serde_json::to_vec_pretty(&settings)?)?;
            cleaned.push("legacy Antigravity --source gemini Stop hook");
        }
    }

    let legacy_path = resolve_legacy_gemini_settings();
    if legacy_path.is_file() {
        let mut settings = read_settings(&legacy_path)?;
        if remove_legacy_settings_command(&mut settings, hook)? {
            backup_file(
                &legacy_path,
                &app.paths.backups_dir,
                "legacy-gemini-settings-cleanup",
            )?;
            fs::write(&legacy_path, serde_json::to_vec_pretty(&settings)?)?;
            cleaned.push("legacy Gemini CLI SessionEnd hook");
        }
    }

    Ok(if cleaned.is_empty() {
        String::new()
    } else {
        format!("; cleaned={}", cleaned.join(", "))
    })
}

fn antigravity_event_has_command(settings: &Value, event: &str, command: &str) -> bool {
    settings
        .get(event)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .any(|entry| direct_entry_matches(entry, command))
        })
        .unwrap_or(false)
}

fn direct_entry_matches(entry: &Value, command: &str) -> bool {
    entry.get("command").and_then(Value::as_str) == Some(command)
}

fn is_llmusage_owned_legacy_gemini_command(command: &str) -> bool {
    command.contains("llmusage-hook") && command.contains("--source gemini")
}

fn ensure_antigravity_event_command(
    settings: &mut Value,
    event: &str,
    command: &str,
) -> Result<()> {
    let root = root_object_mut(settings, "Antigravity hooks.json")?;
    let entries = root.entry(event.to_string()).or_insert_with(|| json!([]));
    let array = entries
        .as_array_mut()
        .ok_or_else(|| anyhow!("Antigravity hooks.{event} must be an array"))?;

    if !array
        .iter()
        .any(|entry| direct_entry_matches(entry, command))
    {
        array.push(json!({ "type": "command", "command": command }));
    }
    Ok(())
}

fn remove_antigravity_event_command(
    settings: &mut Value,
    event: &str,
    command: &str,
) -> Result<bool> {
    let Some(entries_value) = root_object_mut(settings, "Antigravity hooks.json")?.get_mut(event)
    else {
        return Ok(false);
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Antigravity hooks.{event} must be an array"))?;
    let before = entries.len();
    entries.retain(|entry| !direct_entry_matches(entry, command));
    Ok(entries.len() != before)
}

fn remove_legacy_antigravity_command(settings: &mut Value) -> Result<bool> {
    let Some(entries_value) =
        root_object_mut(settings, "Antigravity hooks.json")?.get_mut(ANTIGRAVITY_HOOK_EVENT)
    else {
        return Ok(false);
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Antigravity hooks.{ANTIGRAVITY_HOOK_EVENT} must be an array"))?;
    let before = entries.len();
    entries.retain(|entry| {
        !entry
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(is_llmusage_owned_legacy_gemini_command)
    });
    Ok(entries.len() != before)
}

fn remove_legacy_settings_command(settings: &mut Value, hook: &HookTarget) -> Result<bool> {
    let legacy_commands = [
        hook.shell_command(SourceKind::Antigravity, LEGACY_GEMINI_HOOK_EVENT),
        hook.shell_command(SourceKind::Antigravity, ANTIGRAVITY_HOOK_EVENT),
    ];
    remove_legacy_settings_command_with_matches(settings, &legacy_commands)
}

fn remove_legacy_settings_command_with_matches(
    settings: &mut Value,
    legacy_commands: &[String],
) -> Result<bool> {
    let Some(hooks_value) = root_object_mut(settings, "Gemini settings.json")?.get_mut("hooks")
    else {
        return Ok(false);
    };
    let hooks = hooks_value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Gemini settings.json hooks field must be an object"))?;
    let Some(entries_value) = hooks.get_mut(LEGACY_GEMINI_HOOK_EVENT) else {
        return Ok(false);
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Gemini hooks.{LEGACY_GEMINI_HOOK_EVENT} must be an array"))?;
    let before = entries.len();
    entries.retain(|entry| !legacy_settings_entry_is_llmusage_owned(entry, legacy_commands));
    Ok(entries.len() != before)
}

fn legacy_settings_entry_is_llmusage_owned(entry: &Value, legacy_commands: &[String]) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| {
                        legacy_commands.iter().any(|legacy| legacy == command)
                            || is_llmusage_owned_legacy_gemini_command(command)
                    })
            })
        })
        .unwrap_or(false)
}

fn root_object_mut<'a>(settings: &'a mut Value, label: &str) -> Result<&'a mut Map<String, Value>> {
    settings
        .as_object_mut()
        .ok_or_else(|| anyhow!("{label} top level must be an object"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_install_idempotent() -> Result<()> {
        let mut hooks = json!({});
        let command = "llmusage-hook --source antigravity --trigger Stop --auto";

        ensure_antigravity_event_command(&mut hooks, ANTIGRAVITY_HOOK_EVENT, command)?;
        ensure_antigravity_event_command(&mut hooks, ANTIGRAVITY_HOOK_EVENT, command)?;

        let entries = hooks
            .get(ANTIGRAVITY_HOOK_EVENT)
            .and_then(Value::as_array)
            .expect("Stop entries should exist");
        let matching = entries
            .iter()
            .filter(|entry| direct_entry_matches(entry, command))
            .count();
        assert_eq!(matching, 1);
        assert_eq!(
            entries[0].get("type").and_then(Value::as_str),
            Some("command")
        );
        Ok(())
    }

    #[test]
    fn antigravity_cleanup_removes_legacy_gemini_command_only() -> Result<()> {
        let user_command = "/usr/local/bin/notify --title agy";
        let legacy_command = "llmusage-hook --source gemini --trigger Stop --auto";
        let current_command = "llmusage-hook --source antigravity --trigger Stop --auto";
        let mut hooks = json!({
            ANTIGRAVITY_HOOK_EVENT: [
                { "type": "command", "command": user_command },
                { "type": "command", "command": legacy_command },
                { "type": "command", "command": current_command },
            ]
        });

        assert!(remove_legacy_antigravity_command(&mut hooks)?);

        let entries = hooks
            .get(ANTIGRAVITY_HOOK_EVENT)
            .and_then(Value::as_array)
            .expect("Stop entries should still exist");
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .any(|entry| direct_entry_matches(entry, user_command))
        );
        assert!(
            entries
                .iter()
                .any(|entry| direct_entry_matches(entry, current_command))
        );
        assert!(
            !entries
                .iter()
                .any(|entry| direct_entry_matches(entry, legacy_command))
        );
        Ok(())
    }

    #[test]
    fn legacy_settings_cleanup_preserves_user_session_end_hooks() -> Result<()> {
        let user_command = "/usr/local/bin/notify --title gemini";
        let llmusage_command = "llmusage-hook --source gemini --trigger SessionEnd --auto";
        let mut settings = json!({
            "hooks": {
                LEGACY_GEMINI_HOOK_EVENT: [
                    { "hooks": [{ "type": "command", "command": user_command }] },
                    { "hooks": [{ "type": "command", "command": llmusage_command }] },
                ]
            }
        });

        let changed = remove_legacy_settings_command_with_matches(&mut settings, &[])?;
        assert!(changed);

        let entries = settings
            .get("hooks")
            .and_then(|hooks| hooks.get(LEGACY_GEMINI_HOOK_EVENT))
            .and_then(Value::as_array)
            .expect("SessionEnd entries should still exist");
        assert_eq!(entries.len(), 1);
        let remaining = entries[0]
            .get("hooks")
            .and_then(Value::as_array)
            .and_then(|hooks| hooks[0].get("command"))
            .and_then(Value::as_str);
        assert_eq!(remaining, Some(user_command));
        Ok(())
    }
}
