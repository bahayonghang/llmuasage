use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use tracing::info;

use crate::{app::AppContext, models::SourceKind, registry, store::Store, util::now_utc};

pub mod claude;
pub mod codex;
pub mod gemini;
pub mod hook_target;
pub mod integration;
pub mod opencode;

pub use hook_target::{HookKind, HookTarget};
pub use integration::Integration;

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationProbe {
    pub source: SourceKind,
    pub status: String,
    pub detail: String,
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationAction {
    pub source: SourceKind,
    pub status: String,
    pub detail: String,
}

pub fn probe_all(app: &AppContext) -> Result<Vec<IntegrationProbe>> {
    registry::registered_integrations()
        .iter()
        .map(|integ| integ.probe(app))
        .collect()
}

pub fn install_all(app: &AppContext, store: &Store) -> Result<Vec<IntegrationAction>> {
    /*
     * ========================================================================
     * 步骤1：生成本地 hook 包装器并安装注册的所有集成
     * ========================================================================
     * 目标：
     * 1) 先生成 Windows / Unix 两类 hook 包装器
     * 2) 再按 registry::registered_integrations 顺序遍历安装
     * 3) 每个集成的安装结果都写入 integration_install
     */
    info!("开始生成本地 hook 包装器并安装集成");

    // 1.1 先生成本地 hook 包装器
    write_hook_wrappers(app)?;

    // 1.2 遍历注册表安装每个集成
    let mut actions = Vec::new();
    for integ in registry::registered_integrations() {
        let source = integ.source();
        let action = collect_install_result(store, source, integ.install(app, store))?;
        actions.push(action);
    }

    info!("完成本地 hook 包装器生成与集成安装");
    Ok(actions)
}

pub fn uninstall_all(app: &AppContext, store: &Store) -> Result<Vec<IntegrationAction>> {
    registry::registered_integrations()
        .iter()
        .map(|integ| integ.uninstall(app, store))
        .collect()
}

pub fn write_hook_wrappers(app: &AppContext) -> Result<()> {
    let cmd_body = format!(
        "@echo off\r\n\"{}\" hook-run %*\r\n",
        app.current_exe.to_string_lossy()
    );
    let sh_body = format!(
        "#!/usr/bin/env sh\n\"{}\" hook-run \"$@\"\n",
        app.current_exe.to_string_lossy().replace('"', "\\\"")
    );

    fs::create_dir_all(&app.paths.bin_dir)?;
    fs::write(&app.paths.hook_cmd_path, cmd_body)?;
    fs::write(&app.paths.hook_sh_path, sh_body)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&app.paths.hook_sh_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&app.paths.hook_sh_path, perms)?;
    }

    Ok(())
}

pub fn backup_file(original: &Path, backups_dir: &Path, stem: &str) -> Result<PathBuf> {
    fs::create_dir_all(backups_dir)?;
    let backup_path = backups_dir.join(format!("{stem}.{}.bak", now_utc().replace(':', "-")));
    fs::copy(original, &backup_path)?;
    Ok(backup_path)
}

pub fn record_probe(store: &Store, probe: &IntegrationProbe) -> Result<()> {
    Ok(store.integration_state().record_integration_state(
        probe.source,
        "probe",
        &probe.status,
        probe.config_path.as_deref().map(Path::new),
        None,
        Some(&json!({ "detail": probe.detail })),
    )?)
}

pub fn record_action(
    store: &Store,
    source: SourceKind,
    install_type: &str,
    status: &str,
    detail: &str,
    config_path: Option<&Path>,
    backup_path: Option<&Path>,
) -> Result<()> {
    Ok(store.integration_state().record_integration_state(
        source,
        install_type,
        status,
        config_path,
        backup_path,
        Some(&json!({ "detail": detail })),
    )?)
}

fn collect_install_result(
    store: &Store,
    source: SourceKind,
    result: Result<IntegrationAction>,
) -> Result<IntegrationAction> {
    match result {
        Ok(action) => Ok(action),
        Err(err) => {
            let detail = format!("{err:#}");
            record_action(store, source, "init", "error", &detail, None, None)?;
            Ok(IntegrationAction {
                source,
                status: "error".to_string(),
                detail,
            })
        }
    }
}
