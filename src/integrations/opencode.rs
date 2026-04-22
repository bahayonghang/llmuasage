use std::{fs, path::PathBuf};

use anyhow::Result;

use crate::{app::AppContext, models::SourceKind, store::Store};

use super::{
    IntegrationAction, IntegrationProbe, backup_file, platform_shell_command, record_action,
};

const PLUGIN_MARKER: &str = "LLMUSAGE_LOCAL_PLUGIN";
const PLUGIN_NAME: &str = "llmusage-tracker.js";

pub fn probe(app: &AppContext) -> Result<IntegrationProbe> {
    let plugin_path = resolve_plugin_path(app);
    let probe = if !plugin_path.exists() {
        IntegrationProbe {
            source: SourceKind::Opencode,
            status: "missing".to_string(),
            detail: "OpenCode plugin 不存在".to_string(),
            config_path: Some(plugin_path.to_string_lossy().to_string()),
        }
    } else {
        let content = fs::read_to_string(&plugin_path)?;
        let ready = content.contains(PLUGIN_MARKER);
        IntegrationProbe {
            source: SourceKind::Opencode,
            status: if ready { "ready" } else { "drifted" }.to_string(),
            detail: if ready {
                "OpenCode plugin 已对齐".to_string()
            } else {
                "OpenCode plugin 需要重装".to_string()
            },
            config_path: Some(plugin_path.to_string_lossy().to_string()),
        }
    };
    Ok(probe)
}

pub fn install(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let plugin_path = resolve_plugin_path(app);
    if let Some(parent) = plugin_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let backup_path = if plugin_path.is_file() {
        Some(backup_file(
            &plugin_path,
            &app.paths.backups_dir,
            "opencode-plugin",
        )?)
    } else {
        None
    };

    fs::write(&plugin_path, build_plugin(app))?;
    record_action(
        store,
        SourceKind::Opencode,
        "init",
        "ready",
        "OpenCode plugin 已安装",
        Some(&plugin_path),
        backup_path.as_deref(),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Opencode,
        status: "ready".to_string(),
        detail: "OpenCode plugin 已安装".to_string(),
    })
}

pub fn uninstall(app: &AppContext, store: &Store) -> Result<IntegrationAction> {
    let plugin_path = resolve_plugin_path(app);
    if !plugin_path.exists() {
        return Ok(IntegrationAction {
            source: SourceKind::Opencode,
            status: "skipped".to_string(),
            detail: "OpenCode plugin 不存在".to_string(),
        });
    }

    let backup_path = backup_file(
        &plugin_path,
        &app.paths.backups_dir,
        "opencode-plugin-restore",
    )?;
    let content = fs::read_to_string(&plugin_path)?;
    if content.contains(PLUGIN_MARKER) {
        fs::remove_file(&plugin_path)?;
    }

    record_action(
        store,
        SourceKind::Opencode,
        "uninstall",
        "restored",
        "OpenCode plugin 已移除",
        Some(&plugin_path),
        Some(&backup_path),
    )?;

    Ok(IntegrationAction {
        source: SourceKind::Opencode,
        status: "restored".to_string(),
        detail: "OpenCode plugin 已移除".to_string(),
    })
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

fn build_plugin(app: &AppContext) -> String {
    let command = platform_shell_command(app, SourceKind::Opencode, "session.updated");
    format!(
        "// {PLUGIN_MARKER}\n\
         export const LlmusagePlugin = async ({{ $ }}) => {{\n\
           return {{\n\
             event: async ({{ event }}) => {{\n\
               if (!event || event.type !== 'session.updated') return;\n\
               try {{\n\
                 const proc = $`{command}`;\n\
                 if (proc && typeof proc.catch === 'function') proc.catch(() => {{}});\n\
               }} catch (_) {{}}\n\
             }}\n\
           }};\n\
         }};\n"
    )
}
