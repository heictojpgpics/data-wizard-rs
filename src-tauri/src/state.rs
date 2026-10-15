//! Process-wide application state.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

use dw_core::{ProgressSink, ScanHandle};

pub struct AppState {
    pub scans: HashMap<Uuid, ScanRecord>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scans: HashMap::new(),
        }
    }
}

pub struct ScanRecord {
    pub handle: ScanHandle,
    pub progress: Arc<dyn ProgressSink>,
}

impl ScanRecord {
    pub fn new(handle: ScanHandle) -> Self {
        let (_sink, _rx) = dw_core::progress::BroadcastSink::new(handle.id(), 64);
        // NOTE: For now we use a no-op sink. The Tauri command layer will
        //       wire this up to forward events to the webview in a follow-up.
        struct NoOpSink;
        #[async_trait::async_trait]
        impl ProgressSink for NoOpSink {
            async fn emit(&self, _e: dw_core::ProgressEvent) {}
            async fn fail(&self, _m: String) {}
            async fn done(&self) {}
        }
        Self {
            handle,
            progress: Arc::new(NoOpSink),
        }
    }
}

// Silence unused-import warning for RwLock (used implicitly via State).
#[allow(dead_code)]
fn _unused() -> Arc<RwLock<()>> {
    Arc::new(RwLock::new(()))
}
