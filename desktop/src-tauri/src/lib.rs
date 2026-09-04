pub mod commands;
pub mod dto;
pub mod error;
pub mod state;
pub mod supervisor;

use tauri::Manager;

pub use dto::{
    CancelQueriesDto, ExplorerDto, FilterDto, InteractiveRequest, LogsDto, PrefsDto, QuotaResponse,
    RuntimeInfoDto, SecondaryRequest, SyncStartDto, TopSessionsDto, convert_explorer,
    convert_filter, convert_logs, convert_sync, convert_top_sessions, convert_window,
};
pub use error::{DesktopError, map_llmusage_error};
pub use state::{AppState, startup, startup_from_root};
pub use supervisor::{DesktopQuerySupervisor, SupervisorSnapshot, run_query};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let state = tauri::async_runtime::block_on(startup_from_root(None))?;
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event
                && let Some(state) = window.try_state::<AppState>()
            {
                state.supervisor.cancel_all();
                for job in state.jobs.list_recent(32) {
                    let _ = state.jobs.cancel(&job.job_id);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::runtime_info,
            commands::dashboard_interactive,
            commands::home_overview,
            commands::heatmap,
            commands::trends_daily,
            commands::hour_of_week,
            commands::top_sessions,
            commands::activity,
            commands::tools,
            commands::optimize,
            commands::compare,
            commands::explorer,
            commands::logs,
            commands::diagnostics,
            commands::start_sync,
            commands::job_snapshot,
            commands::cancel_job,
            commands::cancel_queries,
            commands::fetch_quota,
            commands::load_prefs,
            commands::save_prefs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running llmusage desktop");
}
