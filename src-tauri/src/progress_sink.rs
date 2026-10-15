//! Tauri-specific [`ProgressSink`] that forwards events to the webview.

use async_trait::async_trait;
use tauri::{AppHandle, Emitter};
use tracing::debug;
use uuid::Uuid;

use dw_core::{ProgressEvent, ProgressPhase, ProgressSink};
use dw_ipc::types::{ScanPhase, ScanProgress};

/// A [`ProgressSink`] that emits Tauri events to the webview.
///
/// Events are emitted on the `scan:progress` channel. The UI listens
/// via `@tauri-apps/api/event` and updates the progress bar live.
pub struct TauriProgressSink {
    app: AppHandle,
    scan_id: Uuid,
}

impl TauriProgressSink {
    #[must_use]
    pub fn new(app: AppHandle, scan_id: Uuid) -> Self {
        Self { app, scan_id }
    }
}

#[async_trait]
impl ProgressSink for TauriProgressSink {
    async fn emit(&self, event: ProgressEvent) {
        let dto = ScanProgress {
            scan_id: self.scan_id.to_string(),
            phase: map_phase(event.phase),
            current: event.current,
            total: event.total,
            message: event.message,
        };
        if let Err(e) = self.app.emit("scan:progress", &dto) {
            debug!(error = %e, "failed to emit scan:progress event");
        }
    }

    async fn fail(&self, message: String) {
        let dto = ScanProgress {
            scan_id: self.scan_id.to_string(),
            phase: ScanPhase::Failed,
            current: 0,
            total: 0,
            message,
        };
        let _ = self.app.emit("scan:progress", &dto);
    }

    async fn done(&self) {
        let dto = ScanProgress {
            scan_id: self.scan_id.to_string(),
            phase: ScanPhase::Done,
            current: 0,
            total: 0,
            message: String::new(),
        };
        let _ = self.app.emit("scan:progress", &dto);
    }
}

fn map_phase(phase: ProgressPhase) -> ScanPhase {
    match phase {
        ProgressPhase::Probing => ScanPhase::Probing,
        ProgressPhase::QuickScan => ScanPhase::QuickScan,
        ProgressPhase::DeepScan => ScanPhase::DeepScan,
        ProgressPhase::Carving => ScanPhase::Carving,
        ProgressPhase::Finalizing => ScanPhase::Finalizing,
        ProgressPhase::Done => ScanPhase::Done,
        ProgressPhase::Cancelled => ScanPhase::Cancelled,
        ProgressPhase::Failed => ScanPhase::Failed,
    }
}
