//! Tauri command definitions (request/response pairs).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::types::{DiskInfo, PartitionInfo, RecoveredFile, ScanProgress};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListDisksRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListDisksResponse {
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StartScanRequest {
    pub source_uri: String,
    pub deep: bool,
    pub max_results: u32,
    pub file_filter: Vec<String>,
    /// Optional partition index within the source (from list_partitions).
    /// When set, the scan runs against that partition only (reads are
    /// translated by the partition offset); when None, the whole source
    /// is scanned as one volume.
    #[serde(default)]
    pub partition: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StartScanResponse {
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CancelScanRequest {
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CancelScanResponse {
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GetScanProgressRequest {
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GetScanProgressResponse {
    pub progress: ScanProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListResultsRequest {
    pub scan_id: String,
    pub offset: u32,
    pub limit: u32,
    pub extension_filter: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListResultsResponse {
    pub total: u64,
    pub spilled: u64,
    pub items: Vec<RecoveredFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListPartitionsRequest {
    pub source_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListPartitionsResponse {
    /// "gpt" | "mbr" | "none"
    pub table_kind: String,
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecoverFilesRequest {
    pub scan_id: String,
    pub entry_ids: Vec<String>,
    pub destination_dir: String,
    /// "error" (refuse collisions), "keep_both" (write "name (n).ext"),
    /// "overwrite" (replace). Default when absent: "keep_both" — the
    /// never-destroys-user-data choice for interactive recovery.
    pub collision_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RecoverFilesResponse {
    pub recovered: u32,
    pub failed: u32,
    pub bytes_written: u64,
    pub results: Vec<super::types::FileRecoveryOutcome>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanStatsRequest {
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanStatsResponse {
    pub stats: super::types::ScanStatsDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SaveScanRequest {
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SaveScanResponse {
    pub saved: bool,
    pub entries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SavedScanSummary {
    pub scan_id: String,
    pub source_uri: String,
    pub created_at: String,
    /// "quick" | "deep"
    pub mode: String,
    pub state: String,
    pub entries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListSavedScansRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ListSavedScansResponse {
    pub scans: Vec<SavedScanSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResumeScanRequest {
    /// The scan_id of a saved checkpoint.
    pub saved_scan_id: String,
    /// Continue with a deep pass?
    pub deep: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResumeScanResponse {
    /// The scan_id of the NEW running scan (seeded with the checkpoint's
    /// results).
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DeleteSavedScanRequest {
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DeleteSavedScanResponse {
    pub deleted: bool,
}
