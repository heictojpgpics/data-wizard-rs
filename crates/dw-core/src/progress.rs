//! Progress event types.

use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub scan_id: uuid::Uuid,
    pub phase: ProgressPhase,
    pub current: u64,
    pub total: u64,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProgressPhase {
    Probing,
    QuickScan,
    DeepScan,
    Carving,
    Finalizing,
    Done,
    Cancelled,
    Failed,
}

/// Sink for progress events. The Tauri command layer implements this and
/// forwards events to the webview.
#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn emit(&self, event: ProgressEvent);
    async fn fail(&self, message: String);
    async fn done(&self);
}

/// In-process sink backed by a `tokio::sync::broadcast` channel. Used by
/// tests and by the headless CLI.
pub struct BroadcastSink {
    scan_id: uuid::Uuid,
    tx: broadcast::Sender<ProgressEvent>,
    counter: AtomicU64,
}

impl BroadcastSink {
    #[must_use]
    pub fn new(scan_id: uuid::Uuid, capacity: usize) -> (Self, broadcast::Receiver<ProgressEvent>) {
        let (tx, rx) = broadcast::channel(capacity);
        (
            Self {
                scan_id,
                tx,
                counter: AtomicU64::new(0),
            },
            rx,
        )
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ProgressEvent> {
        self.tx.subscribe()
    }
}

#[async_trait]
impl ProgressSink for BroadcastSink {
    async fn emit(&self, event: ProgressEvent) {
        self.counter.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(event);
    }

    async fn fail(&self, message: String) {
        let _ = self.tx.send(ProgressEvent {
            scan_id: self.scan_id,
            phase: ProgressPhase::Failed,
            current: self.counter.load(Ordering::Relaxed),
            total: 0,
            message,
        });
    }

    async fn done(&self) {
        let _ = self.tx.send(ProgressEvent {
            scan_id: self.scan_id,
            phase: ProgressPhase::Done,
            current: self.counter.load(Ordering::Relaxed),
            total: 0,
            message: String::new(),
        });
    }
}
