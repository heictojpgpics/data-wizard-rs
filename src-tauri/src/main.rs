// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod progress_sink;
mod state;

use std::sync::Arc;

use parking_lot::RwLock;
use tauri::Manager;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::state::AppState;

fn main() {
    // Initialize tracing
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,dw_core=debug,dw_tauri=debug"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true))
        .init();

    tracing::info!("Data Wizard starting up");

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::list_disks,
            commands::list_partitions,
            commands::start_scan,
            commands::save_scan,
            commands::list_saved_scans,
            commands::resume_scan,
            commands::delete_saved_scan,
            commands::cancel_scan,
            commands::get_scan_progress,
            commands::list_results,
            commands::scan_stats,
            commands::recover_files,
        ])
        .setup(|app| {
            // Create the app state with the AppHandle (needed for
            // TauriProgressSink to emit events to the webview).
            let state = Arc::new(RwLock::new(AppState::new(app.handle().clone())));
            app.manage(state);
            tracing::info!("Tauri setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
