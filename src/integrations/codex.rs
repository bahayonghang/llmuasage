use std::{fs, path::PathBuf};

use anyhow::Result;
use serde_json::json;
use toml_edit::{DocumentMut, Item, Value, value};

use crate::{app::AppContext, models::SourceKind, store::Store, util::resolve_home_dir};

use super::{
    IntegrationAction, IntegrationProbe, backup_file, platform_notify_args, record_action,
    record_probe,
};

pub fn probe(app: &AppContext) -> Result<IntegrationProbe> {
    let config_path = resolve_codex_config(app);
    let expected = platform_notify_args(app, SourceKind::Codex, "notify");

    let probe = if !config_path.is_file() {
        IntegrationProbe {
            source: SourceKind::Codex,
            status: "missing".to_string(),
            detail: "Codex config.toml 不存在".to_string(),
            config_path: Some(config_path.to_string_lossy().to_string()),
        }
    } else {
        let current = read_notify(&config_path)?;
        let matches = current
            .as_ref()
            .map(|value| value == &expected)
            .unwrap_or(false);
        IntegrationProbe {
            source: SourceKind::Codex,
            status: if matches { "ready" } else { "drifted" }.to_string(),
            detail: if matches {
                "Codex notify 已对齐".to_string()
            } else {
                "Codex notify 需要重装".to_string()
            },
            config_path: Some(config_path.to_string_lossy().to_string()),
        }
    };

    Ok(probe)
}

pub fn install(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let config_path = resolve_codex_config(app);
    let probe = probe(app)?;
    if probe.status == "missing" {
        record_probe(store, &probe)?;
        return Ok(IntegrationAction {
            source: SourceKind::Codex,
            status: "skipped".to_string(),
            detail: "Codex config.toml 缺失，跳过安装".to_string(),
        });
    }

    let expected = platform_notify_args(app, SourceKind::Codex, "notify");
    let raw = fs::read_to_string(&config_path)?;
    let mut doc = raw.parse::<DocumentMut>()?;
    let current = read_notify(&config_path)?;
    let backup_value_path = app.paths.backups_dir.join("codex_notify_original.json");

    if let Some(current) = current.as_ref()
        && current != &expected
        && !backup_value_path.exists()
    {
        fs::write(
            &backup_value_path,
            serde_json::to_vec_pretty(&json!({ "notify": current }))?,
        )?;
    }

    let backup_path = backup_file(&config_path, &app.paths.backups_dir, "codex-config")?;
    let notify_array = expected
        .iter()
        .map(|entry| Value::from(entry.as_str()))
        .collect::<toml_edit::Array>();
    doc["notify"] = value(notify_array);
    fs::write(&config_path, doc.to_string())?;

    record_action(
        store,
        SourceKind::Codex,
        "init",
        "ready",
        "Codex notify 已安装",
        Some(&config_path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Codex,
        status: "ready".to_string(),
        detail: "Codex notify 已安装".to_string(),
    })
}

pub fn uninstall(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let config_path = resolve_codex_config(app);
    if !config_path.is_file() {
        return Ok(IntegrationAction {
            source: SourceKind::Codex,
            status: "skipped".to_string(),
            detail: "Codex config.toml 不存在".to_string(),
        });
    }

    let raw = fs::read_to_string(&config_path)?;
    let mut doc = raw.parse::<DocumentMut>()?;
    let backup_path = backup_file(&config_path, &app.paths.backups_dir, "codex-config-restore")?;
    let backup_value_path = app.paths.backups_dir.join("codex_notify_original.json");

    if backup_value_path.exists() {
        let backup_json: serde_json::Value =
            serde_json::from_slice(&fs::read(&backup_value_path)?)?;
        if let Some(notify_values) = backup_json.get("notify").and_then(|value| value.as_array()) {
            let restored = notify_values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .map(|value| Value::from(value.as_str()))
                .collect::<toml_edit::Array>();
            doc["notify"] = value(restored);
        } else {
            doc.remove("notify");
        }
    } else {
        doc.remove("notify");
    }

    fs::write(&config_path, doc.to_string())?;
    record_action(
        store,
        SourceKind::Codex,
        "uninstall",
        "restored",
        "Codex notify 已恢复",
        Some(&config_path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Codex,
        status: "restored".to_string(),
        detail: "Codex notify 已恢复".to_string(),
    })
}

fn resolve_codex_config(_app: &AppContext) -> PathBuf {
    let home_dir = resolve_home_dir();
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir.join(".codex"))
        .join("config.toml")
}

fn read_notify(config_path: &PathBuf) -> Result<Option<Vec<String>>> {
    let raw = fs::read_to_string(config_path)?;
    let doc = raw.parse::<DocumentMut>()?;
    let notify = doc.get("notify").and_then(Item::as_array).map(|array| {
        array
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect::<Vec<_>>()
    });
    Ok(notify)
}
