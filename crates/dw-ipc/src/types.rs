//! Types shared between the Rust core and the React/TS UI.
//!
//! All UUIDs are serialized as `String` on the wire (standard JSON
//! practice). The Tauri command layer converts to/from `uuid::Uuid`
//! when calling into the Rust core.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DiskInfo {
    pub id: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub is_removable: bool,
    pub is_system: bool,
    pub bus_type: BusType,
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum BusType {
    Unknown,
    Sata,
    Nvme,
    Usb,
    Sd,
    Mmc,
    Virtual,
    IScsi,
    FibreChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PartitionInfo {
    pub index: u32,
    pub offset_bytes: u64,
    pub size_bytes: u64,
    pub fs_kind: Option<String>,
    pub label: Option<String>,
    pub drive_letter: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecoveredFile {
    pub id: String,
    pub name: String,
    pub parent_path: String,
    pub size_bytes: u64,
    pub mtime: Option<String>,
    pub status: RecoveryStatusDto,
    pub extension: String,
    pub mime: Option<String>,
    pub confidence: u16,
    pub was_deleted: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum RecoveryStatusDto {
    Intact,
    PartiallyRecovered,
    Overwritten,
    Carved,
    PendingValidation,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum ScanPhase {
    Probing,
    QuickScan,
    DeepScan,
    Carving,
    Finalizing,
    Done,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanProgress {
    pub scan_id: String,
    pub phase: ScanPhase,
    pub current: u64,
    pub total: u64,
    pub message: String,
}
