// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod diagnostics;
mod progress_sink;
mod state;

use std::sync::Arc;

use parking_lot::RwLock;
use tauri::Manager;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::state::AppState;

fn main() {
    // Headless self-check channel: `dw-tauri.exe --diagnostics` prints
    // a JSON report of the storage stack (drives, sizes, volumes, raw
    // access) and exits — used by the CI installer smoke test and for
    // user support. No window, no Tauri runtime.
    if std::env::args().any(|a| a.eq_ignore_ascii_case("--diagnostics")) {
        std::process::exit(diagnostics::run());
    }

    // Initialize tracing
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,dw_core=debug,dw_tauri=debug"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true))
        .init();

    tracing::info!("Data Wizard starting up");

    tauri::Builder::default()
        // NOTE: no tauri_plugin_log here. The tracing subscriber installed
        // above already carries the `log`-crate bridge (tracing-subscriber's
        // default `tracing-log` feature), so a second log installation would
        // fail with PluginInitialization("log", ...) and kill the app at
        // startup. All log-crate records are forwarded into tracing instead.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
            commands::set_scan_paused,
            commands::get_scan_progress,
            commands::list_results,
            commands::scan_stats,
            commands::recover_files,
            commands::get_dev_hooks,
        ])
        .setup(|app| {
            // Create the app state with the AppHandle (needed for
            // TauriProgressSink to emit events to the webview).
            let state = Arc::new(RwLock::new(AppState::new(app.handle().clone())));
            app.manage(state);

            // Reveal the main window only after it exists (it is created
            // hidden in tauri.conf.json so the first frame lands with the
            // theme CSS applied — no white flash). The optional
            // DATAWIZARD_WINDOW spec (e.g. `1280x800`) sizes the CI
            // screenshot/tour runs; malformed specs fall back to the
            // configured default.
            if let Some(window) = app.get_webview_window("main") {
                if let Some((w, h)) =
                    parse_window_spec(&std::env::var("DATAWIZARD_WINDOW").unwrap_or_default())
                {
                    let _ = window.set_size(tauri::LogicalSize::new(w, h));
                    let _ = window.center();
                }
                let _ = window.show();
            }

            tracing::info!("Tauri setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Parse the `DATAWIZARD_WINDOW` dev hook (`1280x800`, case-insensitive
/// `X` separator). `None` on any malformed spec.
fn parse_window_spec(spec: &str) -> Option<(f64, f64)> {
    let lowered = spec.trim().to_lowercase();
    let (w, h) = lowered.split_once('x')?;
    let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
    (w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then_some((w, h))
}
