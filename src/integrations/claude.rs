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

/// ZST handle implementing [`Integration`] for the Claude `settings.json` hooks.
pub struct ClaudeIntegration;

impl Integration for ClaudeIntegration {
    fn source(&self) -> SourceKind {
        SourceKind::Claude
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
    let settings_path = resolve_claude_settings(app);
    let hook = HookTarget::current(app);
    let stop_command = hook.shell_command(SourceKind::Claude, "Stop");
    let end_command = hook.shell_command(SourceKind::Claude, "SessionEnd");

    let probe = if !settings_path.is_file() {
        IntegrationProbe {
            source: SourceKind::Claude,
            status: "missing".to_string(),
            detail: "Claude settings.json 不存在".to_string(),
            config_path: Some(settings_path.to_string_lossy().to_string()),
        }
    } else {
        let settings = read_settings(&settings_path)?;
        let stop_ready = event_has_command(&settings, "Stop", &stop_command);
        let end_ready = event_has_command(&settings, "SessionEnd", &end_command);
        let ready = stop_ready && end_ready;
        IntegrationProbe {
            source: SourceKind::Claude,
            status: if ready { "ready" } else { "drifted" }.to_string(),
            detail: if ready {
                "Claude hooks 已对齐".to_string()
            } else {
                "Claude hooks 需要重装".to_string()
            },
            config_path: Some(settings_path.to_string_lossy().to_string()),
        }
    };

    Ok(probe)
}

pub fn install(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let settings_path = resolve_claude_settings(app);
    if !settings_path.is_file() {
        let probe = probe(app)?;
        record_probe(store, &probe)?;
        return Ok(IntegrationAction {
            source: SourceKind::Claude,
            status: "skipped".to_string(),
            detail: "Claude settings.json 缺失，跳过安装".to_string(),
        });
    }

    let mut settings = read_settings(&settings_path)?;
    let hook = HookTarget::current(app);
    ensure_event_command(
        &mut settings,
        "Stop",
        &hook.shell_command(SourceKind::Claude, "Stop"),
    )?;
    ensure_event_command(
        &mut settings,
        "SessionEnd",
        &hook.shell_command(SourceKind::Claude, "SessionEnd"),
    )?;

    let backup_path = backup_file(&settings_path, &app.paths.backups_dir, "claude-settings")?;

    fs::write(&settings_path, serde_json::to_vec_pretty(&settings)?)?;
    record_action(
        store,
        SourceKind::Claude,
        "init",
        "ready",
        "Claude hooks 已安装",
        Some(&settings_path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Claude,
        status: "ready".to_string(),
        detail: "Claude hooks 已安装".to_string(),
    })
}

pub fn uninstall(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let settings_path = resolve_claude_settings(app);
    if !settings_path.is_file() {
        return Ok(IntegrationAction {
            source: SourceKind::Claude,
            status: "skipped".to_string(),
            detail: "Claude settings.json 不存在".to_string(),
        });
    }

    let mut settings = read_settings(&settings_path)?;
    let backup_path = backup_file(
        &settings_path,
        &app.paths.backups_dir,
        "claude-settings-restore",
    )?;
    let hook = HookTarget::current(app);
    remove_event_command(
        &mut settings,
        "Stop",
        &hook.shell_command(SourceKind::Claude, "Stop"),
    )?;
    remove_event_command(
        &mut settings,
        "SessionEnd",
        &hook.shell_command(SourceKind::Claude, "SessionEnd"),
    )?;

    fs::write(&settings_path, serde_json::to_vec_pretty(&settings)?)?;
    record_action(
        store,
        SourceKind::Claude,
        "uninstall",
        "restored",
        "Claude hooks 已恢复",
        Some(&settings_path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Claude,
        status: "restored".to_string(),
        detail: "Claude hooks 已恢复".to_string(),
    })
}

fn resolve_claude_settings(_app: &AppContext) -> PathBuf {
    let home_dir = resolve_home_dir();
    home_dir.join(".claude").join("settings.json")
}

fn read_settings(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn event_has_command(settings: &Value, event: &str, command: &str) -> bool {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .map(|entries| {
            entries.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(Value::as_array)
                    .map(|hooks| {
                        hooks.iter().any(|hook| {
                            hook.get("command").and_then(Value::as_str) == Some(command)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn ensure_event_command(settings: &mut Value, event: &str, command: &str) -> Result<()> {
    let hooks = hooks_object_mut(root_object_mut(settings)?)?;
    let array = event_entries_mut(hooks, event)?;

    if !array.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .map(|hooks| {
                hooks
                    .iter()
                    .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command))
            })
            .unwrap_or(false)
    }) {
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
        .ok_or_else(|| anyhow!("Claude settings.json 的 hooks 字段必须是 object"))?;
    let Some(entries_value) = hooks.get_mut(event) else {
        return Ok(());
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Claude hooks.{event} 必须是数组"))?;
    entries.retain(|entry| {
        !entry
            .get("hooks")
            .and_then(Value::as_array)
            .map(|hooks| {
                hooks
                    .iter()
                    .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command))
            })
            .unwrap_or(false)
    });
    Ok(())
}

fn root_object_mut(settings: &mut Value) -> Result<&mut Map<String, Value>> {
    settings
        .as_object_mut()
        .ok_or_else(|| anyhow!("Claude settings.json 顶层必须是 object"))
}

fn hooks_object_mut(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>> {
    let hooks = root.entry("hooks".to_string()).or_insert_with(|| json!({}));
    hooks
        .as_object_mut()
        .ok_or_else(|| anyhow!("Claude settings.json 的 hooks 字段必须是 object"))
}

fn event_entries_mut<'a>(
    hooks: &'a mut Map<String, Value>,
    event: &str,
) -> Result<&'a mut Vec<Value>> {
    let entries = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
    entries
        .as_array_mut()
        .ok_or_else(|| anyhow!("Claude hooks.{event} 必须是数组"))
}
