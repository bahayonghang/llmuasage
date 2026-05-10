use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{app::AppContext, models::SourceKind, store::Store};

use super::{
    HookTarget, Integration, IntegrationAction, IntegrationProbe, backup_file, record_action,
};

const PLUGIN_MARKER: &str = "LLMUSAGE_LOCAL_PLUGIN";
const PLUGIN_NAME: &str = "llmusage-tracker.js";

/// ZST handle implementing [`Integration`] for the OpenCode local plugin.
pub struct OpencodeIntegration;

impl Integration for OpencodeIntegration {
    fn source(&self) -> SourceKind {
        SourceKind::Opencode
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

fn official_storage_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(".local").join("share").join("opencode")
}

fn legacy_storage_dir(home_dir: &Path) -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| home_dir.join(".local").join("share"))
        .join("opencode")
}

pub(crate) fn resolve_default_storage_dir(home_dir: &Path) -> PathBuf {
    let official = official_storage_dir(home_dir);
    if official.exists() {
        return official;
    }

    let legacy = legacy_storage_dir(home_dir);
    if legacy.exists() {
        return legacy;
    }

    official
}

pub(crate) fn resolve_db_path() -> PathBuf {
    std::env::var("OPENCODE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home_dir = crate::util::resolve_home_dir();
            resolve_default_storage_dir(&home_dir)
        })
        .join("opencode.db")
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
    let command = HookTarget::current(app).shell_command(SourceKind::Opencode, "session.updated");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_storage_prefers_official_home_data_dir() {
        let temp = tempfile::tempdir().expect("temp dir");
        let official = temp.path().join(".local").join("share").join("opencode");
        let legacy = temp.path().join("legacy").join("opencode");
        std::fs::create_dir_all(&official).expect("official dir");
        std::fs::create_dir_all(&legacy).expect("legacy dir");

        assert_eq!(resolve_default_storage_dir(temp.path()), official);
    }

    #[test]
    fn default_storage_falls_back_to_official_even_when_absent() {
        let temp = tempfile::tempdir().expect("temp dir");

        assert_eq!(
            resolve_default_storage_dir(temp.path()),
            temp.path().join(".local").join("share").join("opencode"),
        );
    }
}
