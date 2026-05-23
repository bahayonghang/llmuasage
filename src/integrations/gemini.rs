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

const LEGACY_HOOK_EVENT: &str = "SessionEnd";
const ANTIGRAVITY_HOOK_EVENT: &str = "Stop";

/// ZST handle implementing [`Integration`] for Google local CLI hooks.
///
/// The stable llmusage source id remains `gemini`, while installation now
/// supports both legacy Gemini CLI `settings.json::hooks.SessionEnd` and
/// Antigravity CLI `hooks.json::Stop`.
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
    let targets = integration_targets(app);
    let hook = HookTarget::current(app);

    let mut existing = Vec::new();
    let mut ready = Vec::new();
    let mut drifted = Vec::new();
    let mut missing = Vec::new();

    for target in &targets {
        if target.path.is_file() {
            existing.push(target.label);
            let settings = read_settings(&target.path)?;
            if target_has_command(&settings, target, &hook) {
                ready.push(target.label);
            } else {
                drifted.push(target.label);
            }
        } else if target.installable {
            missing.push(target.label);
        }
    }

    let needs_install = drifted
        .iter()
        .chain(missing.iter())
        .copied()
        .collect::<Vec<_>>();

    let (status, detail) = if !ready.is_empty() && needs_install.is_empty() {
        (
            "ready",
            format!(
                "Google Antigravity/Gemini hooks 已对齐: {}",
                ready.join(", ")
            ),
        )
    } else if !ready.is_empty() {
        (
            "partial",
            format!(
                "Google hooks 部分对齐: ready={}, needs-install={}",
                ready.join(", "),
                needs_install.join(", ")
            ),
        )
    } else if !drifted.is_empty() || !missing.is_empty() {
        (
            "drifted",
            format!(
                "Google Antigravity/Gemini hooks 需要安装: {}{}",
                drifted.join(", "),
                if missing.is_empty() {
                    String::new()
                } else {
                    format!("; missing={}", missing.join(", "))
                }
            ),
        )
    } else {
        (
            "missing",
            "未发现 Gemini CLI settings.json 或 Antigravity CLI 配置目录".to_string(),
        )
    };

    Ok(IntegrationProbe {
        source: SourceKind::Gemini,
        status: status.to_string(),
        detail,
        config_path: existing
            .first()
            .and_then(|_| targets.iter().find(|target| target.path.is_file()))
            .map(|target| target.path.to_string_lossy().to_string()),
    })
}

pub fn install(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let targets = integration_targets(app);
    let hook = HookTarget::current(app);
    let mut installed = Vec::new();
    let mut skipped = Vec::new();
    let mut backup_paths = Vec::new();

    for target in &targets {
        if !target.installable {
            skipped.push(target.label);
            continue;
        }

        let mut settings = if target.path.is_file() {
            read_settings(&target.path)?
        } else {
            json!({})
        };
        ensure_target_command(&mut settings, target, &hook)?;

        let backup_path = if target.path.is_file() {
            Some(backup_file(
                &target.path,
                &app.paths.backups_dir,
                target.backup_stem,
            )?)
        } else {
            None
        };

        if let Some(parent) = target.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target.path, serde_json::to_vec_pretty(&settings)?)?;
        if let Some(path) = backup_path {
            backup_paths.push(path);
        }
        installed.push(target.label);
    }

    if installed.is_empty() {
        let probe = probe(app)?;
        record_probe(store, &probe)?;
        return Ok(IntegrationAction {
            source: SourceKind::Gemini,
            status: "skipped".to_string(),
            detail: "未发现 Gemini CLI settings.json 或 Antigravity CLI 配置目录，跳过 Google hooks 安装".to_string(),
        });
    }

    let detail = format!(
        "Google Antigravity/Gemini hooks 已安装: {}{}",
        installed.join(", "),
        if skipped.is_empty() {
            String::new()
        } else {
            format!("; skipped={}", skipped.join(", "))
        }
    );
    let primary_path = targets
        .iter()
        .find(|target| installed.contains(&target.label))
        .map(|target| target.path.as_path());
    record_action(
        store,
        SourceKind::Gemini,
        "init",
        "ready",
        &detail,
        primary_path,
        backup_paths.first().map(|path| path.as_path()),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Gemini,
        status: "ready".to_string(),
        detail,
    })
}

pub fn uninstall(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let targets = integration_targets(app);
    let hook = HookTarget::current(app);
    let mut restored = Vec::new();
    let mut backup_paths = Vec::new();

    for target in &targets {
        if !target.path.is_file() {
            continue;
        }

        let mut settings = read_settings(&target.path)?;
        let backup_path = backup_file(
            &target.path,
            &app.paths.backups_dir,
            target.restore_backup_stem,
        )?;
        remove_target_command(&mut settings, target, &hook)?;
        fs::write(&target.path, serde_json::to_vec_pretty(&settings)?)?;
        backup_paths.push(backup_path);
        restored.push(target.label);
    }

    if restored.is_empty() {
        return Ok(IntegrationAction {
            source: SourceKind::Gemini,
            status: "skipped".to_string(),
            detail: "Gemini/Antigravity hook 配置不存在".to_string(),
        });
    }

    let detail = format!(
        "Google Antigravity/Gemini hooks 已恢复: {}",
        restored.join(", ")
    );
    let primary_path = targets
        .iter()
        .find(|target| restored.contains(&target.label))
        .map(|target| target.path.as_path());
    record_action(
        store,
        SourceKind::Gemini,
        "uninstall",
        "restored",
        &detail,
        primary_path,
        backup_paths.first().map(|path| path.as_path()),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Gemini,
        status: "restored".to_string(),
        detail,
    })
}

#[derive(Clone, Copy)]
enum HookConfigKind {
    LegacyGeminiSettings,
    AntigravityHooks,
}

struct HookConfigTarget {
    label: &'static str,
    kind: HookConfigKind,
    path: PathBuf,
    installable: bool,
    backup_stem: &'static str,
    restore_backup_stem: &'static str,
}

fn integration_targets(app: &AppContext) -> Vec<HookConfigTarget> {
    let home_dir = resolve_home_dir();
    let legacy_path = resolve_gemini_settings(app);
    let antigravity_path = resolve_antigravity_hooks();
    vec![
        HookConfigTarget {
            label: "Gemini CLI SessionEnd",
            kind: HookConfigKind::LegacyGeminiSettings,
            installable: legacy_path.is_file(),
            path: legacy_path,
            backup_stem: "gemini-settings",
            restore_backup_stem: "gemini-settings-restore",
        },
        HookConfigTarget {
            label: "Antigravity CLI Stop",
            kind: HookConfigKind::AntigravityHooks,
            installable: antigravity_path.is_file()
                || home_dir.join(".gemini").join("antigravity-cli").exists()
                || home_dir.join(".gemini").join("antigravity").exists()
                || antigravity_path.parent().is_some_and(Path::exists),
            path: antigravity_path,
            backup_stem: "antigravity-hooks",
            restore_backup_stem: "antigravity-hooks-restore",
        },
    ]
}

fn resolve_gemini_settings(_app: &AppContext) -> PathBuf {
    let home_dir = resolve_home_dir();
    home_dir.join(".gemini").join("settings.json")
}

fn resolve_antigravity_hooks() -> PathBuf {
    let home_dir = resolve_home_dir();
    home_dir.join(".gemini").join("config").join("hooks.json")
}

fn read_settings(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn target_has_command(settings: &Value, target: &HookConfigTarget, hook: &HookTarget) -> bool {
    match target.kind {
        HookConfigKind::LegacyGeminiSettings => event_has_command(
            settings,
            LEGACY_HOOK_EVENT,
            &hook.shell_command(SourceKind::Gemini, LEGACY_HOOK_EVENT),
        ),
        HookConfigKind::AntigravityHooks => antigravity_event_has_command(
            settings,
            ANTIGRAVITY_HOOK_EVENT,
            &hook.shell_command(SourceKind::Gemini, ANTIGRAVITY_HOOK_EVENT),
        ),
    }
}

fn ensure_target_command(
    settings: &mut Value,
    target: &HookConfigTarget,
    hook: &HookTarget,
) -> Result<()> {
    match target.kind {
        HookConfigKind::LegacyGeminiSettings => ensure_event_command(
            settings,
            LEGACY_HOOK_EVENT,
            &hook.shell_command(SourceKind::Gemini, LEGACY_HOOK_EVENT),
        ),
        HookConfigKind::AntigravityHooks => ensure_antigravity_event_command(
            settings,
            ANTIGRAVITY_HOOK_EVENT,
            &hook.shell_command(SourceKind::Gemini, ANTIGRAVITY_HOOK_EVENT),
        ),
    }
}

fn remove_target_command(
    settings: &mut Value,
    target: &HookConfigTarget,
    hook: &HookTarget,
) -> Result<()> {
    match target.kind {
        HookConfigKind::LegacyGeminiSettings => remove_event_command(
            settings,
            LEGACY_HOOK_EVENT,
            &hook.shell_command(SourceKind::Gemini, LEGACY_HOOK_EVENT),
        ),
        HookConfigKind::AntigravityHooks => remove_antigravity_event_command(
            settings,
            ANTIGRAVITY_HOOK_EVENT,
            &hook.shell_command(SourceKind::Gemini, ANTIGRAVITY_HOOK_EVENT),
        ),
    }
}

fn event_has_command(settings: &Value, event: &str, command: &str) -> bool {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .map(|entries| entries.iter().any(|entry| entry_matches(entry, command)))
        .unwrap_or(false)
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

fn entry_matches(entry: &Value, command: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| hooks.iter().any(|hook| direct_entry_matches(hook, command)))
        .unwrap_or(false)
}

fn direct_entry_matches(entry: &Value, command: &str) -> bool {
    entry.get("command").and_then(Value::as_str) == Some(command)
}

fn ensure_event_command(settings: &mut Value, event: &str, command: &str) -> Result<()> {
    let root = root_object_mut(settings, "Gemini settings.json")?;
    let hooks = hooks_object_mut(root, "Gemini settings.json")?;
    let array = event_entries_mut(hooks, event, "Gemini")?;

    if !array.iter().any(|entry| entry_matches(entry, command)) {
        array.push(json!({
            "hooks": [
                { "type": "command", "command": command }
            ]
        }));
    }
    Ok(())
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
        .ok_or_else(|| anyhow!("Antigravity hooks.{event} 必须是数组"))?;

    if !array
        .iter()
        .any(|entry| direct_entry_matches(entry, command))
    {
        array.push(json!({ "type": "command", "command": command }));
    }
    Ok(())
}

fn remove_event_command(settings: &mut Value, event: &str, command: &str) -> Result<()> {
    let Some(hooks_value) = root_object_mut(settings, "Gemini settings.json")?.get_mut("hooks")
    else {
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

fn remove_antigravity_event_command(
    settings: &mut Value,
    event: &str,
    command: &str,
) -> Result<()> {
    let Some(entries_value) = root_object_mut(settings, "Antigravity hooks.json")?.get_mut(event)
    else {
        return Ok(());
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| anyhow!("Antigravity hooks.{event} 必须是数组"))?;
    entries.retain(|entry| !direct_entry_matches(entry, command));
    Ok(())
}

fn root_object_mut<'a>(settings: &'a mut Value, label: &str) -> Result<&'a mut Map<String, Value>> {
    settings
        .as_object_mut()
        .ok_or_else(|| anyhow!("{label} 顶层必须是 object"))
}

fn hooks_object_mut<'a>(
    root: &'a mut Map<String, Value>,
    label: &str,
) -> Result<&'a mut Map<String, Value>> {
    let hooks = root.entry("hooks".to_string()).or_insert_with(|| json!({}));
    hooks
        .as_object_mut()
        .ok_or_else(|| anyhow!("{label} 的 hooks 字段必须是 object"))
}

fn event_entries_mut<'a>(
    hooks: &'a mut Map<String, Value>,
    event: &str,
    source_label: &str,
) -> Result<&'a mut Vec<Value>> {
    let entries = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
    entries
        .as_array_mut()
        .ok_or_else(|| anyhow!("{source_label} hooks.{event} 必须是数组"))
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

        ensure_event_command(&mut settings, LEGACY_HOOK_EVENT, command)?;
        ensure_event_command(&mut settings, LEGACY_HOOK_EVENT, command)?;

        let entries = settings
            .get("hooks")
            .and_then(|hooks| hooks.get(LEGACY_HOOK_EVENT))
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
                LEGACY_HOOK_EVENT: [
                    { "hooks": [{ "type": "command", "command": user_command }] },
                    { "hooks": [{ "type": "command", "command": llmusage_command }] },
                ]
            }
        });

        remove_event_command(&mut settings, LEGACY_HOOK_EVENT, llmusage_command)?;

        let entries = settings
            .get("hooks")
            .and_then(|hooks| hooks.get(LEGACY_HOOK_EVENT))
            .and_then(Value::as_array)
            .expect("SessionEnd entries should still exist");
        assert_eq!(entries.len(), 1);
        assert!(entry_matches(&entries[0], user_command));
        assert!(!entry_matches(&entries[0], llmusage_command));
        Ok(())
    }

    /// Validates Antigravity hooks.json Stop handlers use the documented direct
    /// handler-list shape and do not duplicate llmusage on repeated install.
    #[test]
    fn antigravity_install_idempotent() -> Result<()> {
        let mut hooks = json!({});
        let command = "llmusage-hook --source gemini --trigger Stop --auto";

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

    /// Validates uninstall removes only the llmusage Stop handler and preserves
    /// user-owned Antigravity hooks in the same event list.
    #[test]
    fn antigravity_uninstall_only_removes_own_command() -> Result<()> {
        let user_command = "/usr/local/bin/notify --title agy";
        let llmusage_command = "llmusage-hook --source gemini --trigger Stop --auto";
        let mut hooks = json!({
            ANTIGRAVITY_HOOK_EVENT: [
                { "type": "command", "command": user_command },
                { "type": "command", "command": llmusage_command },
            ]
        });

        remove_antigravity_event_command(&mut hooks, ANTIGRAVITY_HOOK_EVENT, llmusage_command)?;

        let entries = hooks
            .get(ANTIGRAVITY_HOOK_EVENT)
            .and_then(Value::as_array)
            .expect("Stop entries should still exist");
        assert_eq!(entries.len(), 1);
        assert!(direct_entry_matches(&entries[0], user_command));
        assert!(!direct_entry_matches(&entries[0], llmusage_command));
        Ok(())
    }
}
