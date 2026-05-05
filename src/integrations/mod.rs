use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use tracing::info;

use crate::{app::AppContext, models::SourceKind, store::Store, util::now_utc};

pub mod claude;
pub mod codex;
pub mod opencode;

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
    Ok(vec![
        codex::probe(app)?,
        claude::probe(app)?,
        opencode::probe(app)?,
    ])
}

pub fn install_all(app: &AppContext, store: &Store) -> Result<Vec<IntegrationAction>> {
    /*
     * ========================================================================
     * 步骤1：生成本地 hook 包装器并安装三类集成
     * ========================================================================
     * 目标：
     * 1) 先生成 Windows / Unix 两类包装器
     * 2) 再顺序安装 Codex、Claude、OpenCode 集成
     * 3) 所有安装结果都同步写入 integration_install
     */
    info!("开始生成本地 hook 包装器并安装集成");

    // 1.1 先生成本地 hook 包装器
    write_hook_wrappers(app)?;

    // 1.2 顺序安装三类本地集成
    let actions = vec![
        collect_install_result(store, SourceKind::Codex, codex::install(app, store))?,
        collect_install_result(store, SourceKind::Claude, claude::install(app, store))?,
        collect_install_result(store, SourceKind::Opencode, opencode::install(app, store))?,
    ];

    info!("完成本地 hook 包装器生成与集成安装");
    Ok(actions)
}

pub fn uninstall_all(app: &AppContext, store: &Store) -> Result<Vec<IntegrationAction>> {
    Ok(vec![
        codex::uninstall(app, store)?,
        claude::uninstall(app, store)?,
        opencode::uninstall(app, store)?,
    ])
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
    store.record_integration_state(
        probe.source,
        "probe",
        &probe.status,
        probe.config_path.as_deref().map(Path::new),
        None,
        Some(&json!({ "detail": probe.detail })),
    )
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
    store.record_integration_state(
        source,
        install_type,
        status,
        config_path,
        backup_path,
        Some(&json!({ "detail": detail })),
    )
}

pub fn platform_shell_command(app: &AppContext, source: SourceKind, trigger: &str) -> String {
    if cfg!(windows) {
        format!(
            "cmd /c \"{} --source {} --trigger {} --auto\"",
            quote_windows_cmd_path(&app.paths.hook_cmd_path),
            source.as_str(),
            trigger
        )
    } else {
        format!(
            "/usr/bin/env sh {} --source {} --trigger {} --auto",
            quote_unix_path(&app.paths.hook_sh_path),
            source.as_str(),
            trigger
        )
    }
}

pub fn platform_notify_args(app: &AppContext, source: SourceKind, trigger: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd".to_string(),
            "/c".to_string(),
            app.paths.hook_cmd_path.to_string_lossy().to_string(),
            "--source".to_string(),
            source.as_str().to_string(),
            "--trigger".to_string(),
            trigger.to_string(),
            "--auto".to_string(),
        ]
    } else {
        vec![
            "/usr/bin/env".to_string(),
            "sh".to_string(),
            app.paths.hook_sh_path.to_string_lossy().to_string(),
            "--source".to_string(),
            source.as_str().to_string(),
            "--trigger".to_string(),
            trigger.to_string(),
            "--auto".to_string(),
        ]
    }
}

fn quote_unix_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if raw
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "/._-".contains(ch))
    {
        raw.to_string()
    } else {
        format!("\"{}\"", raw.replace('"', "\\\""))
    }
}

fn quote_windows_cmd_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('"', "\"\""))
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
