//! Process-wide application state.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::AppHandle;
use uuid::Uuid;

use dw_core::ScanHandle;

use crate::progress_sink::TauriProgressSink;

pub struct AppState {
    pub app: AppHandle,
    pub scans: HashMap<Uuid, ScanRecord>,
}

impl AppState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            scans: HashMap::new(),
        }
    }
}

pub struct ScanRecord {
    pub handle: ScanHandle,
    /// The source URI (file path or device path) used to open the scan.
    /// Stored so the recovery command can re-open the source to read
    /// file data.
    pub source_uri: String,
    /// The pause gate shared with the `PausableSource` wrapping the
    /// scan's reads. Flipped by `set_scan_paused`; the source stalls
    /// in place (both the sync `ReadAt` view and the async path).
    pub pause: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ScanRecord {
    pub fn new(
        handle: ScanHandle,
        source_uri: String,
        pause: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            handle,
            source_uri,
            pause,
        }
    }

    /// Create a `TauriProgressSink` for this scan.
    pub fn progress_sink(&self, app: &AppHandle) -> Arc<TauriProgressSink> {
        Arc::new(TauriProgressSink::new(app.clone(), self.handle.id()))
    }
}
