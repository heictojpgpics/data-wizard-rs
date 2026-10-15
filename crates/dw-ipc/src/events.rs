//! Event types pushed from the Rust core to the UI.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::types::ScanPhase;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanProgressEvent {
    pub scan_id: String,
    pub phase: ScanPhase,
    pub current: u64,
    pub total: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanFinishedEvent {
    pub scan_id: String,
    pub total_entries: u64,
    pub spilled_entries: u64,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResultsAppendedEvent {
    pub scan_id: String,
    pub first_index: u64,
    pub count: u32,
}
