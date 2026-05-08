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

const HOOK_EVENT: &str = "SessionEnd";

/// ZST handle implementing [`Integration`] for Gemini CLI's
/// `~/.gemini/settings.json::hooks.SessionEnd` array (D14 / F1.1 / P7).
pub struct GeminiIntegration;

impl Integration for GeminiIntegration {
    fn source(&self) -> SourceKind {
        SourceKind::Gemini
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
    let settings_path = resolve_gemini_settings(app);
    let hook = HookTarget::current(app);
    let command = hook.shell_command(SourceKind::Gemini, HOOK_EVENT);

    let probe = if !settings_path.is_file() {
        IntegrationProbe {
            source: SourceKind::Gemini,
            status: "missing".to_string(),
            detail: "Gemini settings.json 不存在".to_string(),
            config_path: Some(settings_path.to_string_lossy().to_string()),
        }
    } else {
        let settings = read_settings(&settings_path)?;
        let ready = event_has_command(&settings, HOOK_EVENT, &command);
        IntegrationProbe {
            source: SourceKind::Gemini,
            status: if ready { "ready" } else { "drifted" }.to_string(),
            detail: if ready {
                "Gemini hooks 已对齐".to_string()
            } else {
                "Gemini hooks 需要重装".to_string()
            },
            config_path: Some(settings_path.to_string_lossy().to_string()),
        }
    };

    Ok(probe)
}

pub fn install(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let settings_path = resolve_gemini_settings(app);
    if !settings_path.is_file() {
        let probe = probe(app)?;
        record_probe(store, &probe)?;
        return Ok(IntegrationAction {
            source: SourceKind::Gemini,
            status: "skipped".to_string(),
            detail: "Gemini settings.json 缺失，跳过安装".to_string(),
        });
    }

    let mut settings = read_settings(&settings_path)?;
    let hook = HookTarget::current(app);
    let command = hook.shell_command(SourceKind::Gemini, HOOK_EVENT);
    ensure_event_command(&mut settings, HOOK_EVENT, &command)?;

    let backup_path = backup_file(&settings_path, &app.paths.backups_dir, "gemini-settings")?;

    fs::write(&settings_path, serde_json::to_vec_pretty(&settings)?)?;
    record_action(
        store,
        SourceKind::Gemini,
        "init",
        "ready",
        "Gemini hooks 已安装",
        Some(&settings_path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Gemini,
        status: "ready".to_string(),
        detail: "Gemini hooks 已安装".to_string(),
    })
}

pub fn uninstall(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let settings_path = resolve_gemini_settings(app);
    if !settings_path.is_file() {
        return Ok(IntegrationAction {
            source: SourceKind::Gemini,
            status: "skipped".to_string(),
            detail: "Gemini settings.json 不存在".to_string(),
        });
    }

    let mut settings = read_settings(&settings_path)?;
    let backup_path = backup_file(
        &settings_path,
        &app.paths.backups_dir,
        "gemini-settings-restore",
    )?;
    let hook = HookTarget::current(app);
    remove_event_command(
        &mut settings,
        HOOK_EVENT,
        &hook.shell_command(SourceKind::Gemini, HOOK_EVENT),
    )?;

    fs::write(&settings_path, serde_json::to_vec_pretty(&settings)?)?;
    record_action(
        store,
        SourceKind::Gemini,
        "uninstall",
        "restored",
        "Gemini hooks 已恢复",
        Some(&settings_path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Gemini,
        status: "restored".to_string(),
        detail: "Gemini hooks 已恢复".to_string(),
    })
}

fn resolve_gemini_settings(_app: &AppContext) -> PathBuf {
    let home_dir = resolve_home_dir();
    home_dir.join(".gemini").join("settings.json")
}

fn read_settings(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn event_has_command(settings: &Value, event: &str, command: &str) -> bool {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .map(|entries| entries.iter().any(|entry| entry_matches(entry, command)))
        .unwrap_or(false)
}

fn entry_matches(entry: &Value, command: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| {
            hooks
                .iter()
                .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command))
        })
        .unwrap_or(false)
}

fn ensure_event_command(settings: &mut Value, event: &str, command: &str) -> Result<()> {
    let root = root_object_mut(settings)?;
    let hooks = hooks_object_mut(root)?;
    let array = event_entries_mut(hooks, event)?;

    if !array.iter().any(|entry| entry_matches(entry, command)) {
        array.push(json!({
            "hooks": [
                { "type": "command", "command": command }
            ]
        }));
    }
    Ok(())
}

fn remove_event_command(settings: &mut Value, event: &str, command: &str) -> Result<()> {
    let Some(hooks_value) = root_object_mut(settings)?.get_mut("hooks") else {
        return Ok(());
    };
    let hooks = hooks_value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Gemini settings.json 的 hooks 字段必须是 object"))?;
    let Some(entries_value) = hooks.get_mut(event) else {
        return Ok(());
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Gemini hooks.{event} 必须是数组"))?;
    entries.retain(|entry| !entry_matches(entry, command));
    Ok(())
}

fn root_object_mut(settings: &mut Value) -> Result<&mut Map<String, Value>> {
    settings
        .as_object_mut()
        .ok_or_else(|| anyhow!("Gemini settings.json 顶层必须是 object"))
}

fn hooks_object_mut(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>> {
    let hooks = root.entry("hooks".to_string()).or_insert_with(|| json!({}));
    hooks
        .as_object_mut()
        .ok_or_else(|| anyhow!("Gemini settings.json 的 hooks 字段必须是 object"))
}

fn event_entries_mut<'a>(
    hooks: &'a mut Map<String, Value>,
    event: &str,
) -> Result<&'a mut Vec<Value>> {
    let entries = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
    entries
        .as_array_mut()
        .ok_or_else(|| anyhow!("Gemini hooks.{event} 必须是数组"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Validates ensure_event_command is idempotent: re-installing twice does
    /// not duplicate the llmusage command in the SessionEnd array.
    #[test]
    fn gemini_install_idempotent() -> Result<()> {
        let mut settings = json!({});
        let command = "llmusage-hook --source gemini --trigger SessionEnd --auto";

        ensure_event_command(&mut settings, HOOK_EVENT, command)?;
        ensure_event_command(&mut settings, HOOK_EVENT, command)?;

        let entries = settings
            .get("hooks")
            .and_then(|hooks| hooks.get(HOOK_EVENT))
            .and_then(Value::as_array)
            .expect("SessionEnd entries should exist");
        let matching = entries
            .iter()
            .filter(|entry| entry_matches(entry, command))
            .count();
        assert_eq!(matching, 1);
        Ok(())
    }

    /// Validates uninstall only removes our entry and preserves user-owned
    /// hook entries that happen to share the same SessionEnd event.
    #[test]
    fn gemini_uninstall_only_removes_own_command() -> Result<()> {
        let user_command = "/usr/local/bin/notify --title gemini";
        let llmusage_command = "llmusage-hook --source gemini --trigger SessionEnd --auto";
        let mut settings = json!({
            "hooks": {
                HOOK_EVENT: [
                    { "hooks": [{ "type": "command", "command": user_command }] },
                    { "hooks": [{ "type": "command", "command": llmusage_command }] },
                ]
            }
        });

        remove_event_command(&mut settings, HOOK_EVENT, llmusage_command)?;

        let entries = settings
            .get("hooks")
            .and_then(|hooks| hooks.get(HOOK_EVENT))
            .and_then(Value::as_array)
            .expect("SessionEnd entries should still exist");
        assert_eq!(entries.len(), 1);
        assert!(entry_matches(&entries[0], user_command));
        assert!(!entry_matches(&entries[0], llmusage_command));
        Ok(())
    }
}
