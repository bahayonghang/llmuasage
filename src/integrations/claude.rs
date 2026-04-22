use std::{fs, path::PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use crate::{app::AppContext, models::SourceKind, store::Store, util::resolve_home_dir};

use super::{
    IntegrationAction, IntegrationProbe, backup_file, platform_shell_command, record_action,
    record_probe,
};

pub fn probe(app: &AppContext) -> Result<IntegrationProbe> {
    let settings_path = resolve_claude_settings(app);
    let stop_command = platform_shell_command(app, SourceKind::Claude, "Stop");
    let end_command = platform_shell_command(app, SourceKind::Claude, "SessionEnd");

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
    let backup_path = backup_file(&settings_path, &app.paths.backups_dir, "claude-settings")?;

    ensure_event_command(
        &mut settings,
        "Stop",
        &platform_shell_command(app, SourceKind::Claude, "Stop"),
    );
    ensure_event_command(
        &mut settings,
        "SessionEnd",
        &platform_shell_command(app, SourceKind::Claude, "SessionEnd"),
    );

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
    remove_event_command(
        &mut settings,
        "Stop",
        &platform_shell_command(app, SourceKind::Claude, "Stop"),
    );
    remove_event_command(
        &mut settings,
        "SessionEnd",
        &platform_shell_command(app, SourceKind::Claude, "SessionEnd"),
    );

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

fn read_settings(path: &PathBuf) -> Result<Value> {
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

fn ensure_event_command(settings: &mut Value, event: &str, command: &str) {
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
    let entries = hooks
        .as_object_mut()
        .unwrap()
        .entry(event.to_string())
        .or_insert_with(|| json!([]));
    let array = entries.as_array_mut().unwrap();

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
}

fn remove_event_command(settings: &mut Value, event: &str, command: &str) {
    if let Some(entries) = settings
        .get_mut("hooks")
        .and_then(|hooks| hooks.get_mut(event))
        .and_then(Value::as_array_mut)
    {
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
    }
}
