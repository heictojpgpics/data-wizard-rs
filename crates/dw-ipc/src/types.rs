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
    /// Free bytes summed over the mounted volumes on this disk, when
    /// known (physical drives with mounted filesystem volumes).
    #[serde(default)]
    pub free_bytes: Option<u64>,
    /// Hardware model string (e.g. `Samsung SSD 970 EVO 1TB`), when the
    /// storage stack reports one.
    #[serde(default)]
    pub model: Option<String>,
    /// Device serial number, when reported.
    #[serde(default)]
    pub serial: Option<String>,
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
    /// Free bytes on the volume mounted at this partition, when one is.
    #[serde(default)]
    pub free_bytes: Option<u64>,
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
    CarvedValidated,
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

/// One category row ("Photos", "Videos", ...).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CategoryCountDto {
    pub category: String,
    pub label: String,
    pub files: u64,
    pub bytes: u64,
}

/// One status row.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StatusCountDto {
    pub status: String,
    pub files: u64,
    pub bytes: u64,
}

/// Aggregated statistics for a scan (drives the results overview).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanStatsDto {
    pub scan_id: String,
    pub total_files: u64,
    pub total_bytes: u64,
    pub by_status: Vec<StatusCountDto>,
    pub by_category: Vec<CategoryCountDto>,
}

/// Per-file outcome of a recovery batch.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FileRecoveryOutcome {
    pub entry_id: String,
    pub name: String,
    pub ok: bool,
    /// Where the file actually landed (KeepBoth may pick "name (2).ext").
    pub path: Option<String>,
    pub bytes_written: u64,
    pub partial: bool,
    pub error: Option<String>,
}

/// Live progress of a recovery batch (event payload).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecoveryProgress {
    pub scan_id: String,
    pub done: u32,
    pub total: u32,
    pub current_file: String,
    pub bytes_written: u64,
}
