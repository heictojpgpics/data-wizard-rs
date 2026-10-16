// IPC types, hand-maintained to mirror dw-ipc (tauri-specta placeholder).
// Keep in sync with crates/dw-ipc/src/{types,commands}.rs.

export type BusType =
  | "Unknown"
  | "Sata"
  | "Nvme"
  | "Usb"
  | "Sd"
  | "Mmc"
  | "Virtual"
  | "IScsi"
  | "FibreChannel";

export interface PartitionInfo {
  index: number;
  offset_bytes: number;
  size_bytes: number;
  free_bytes: number | null;
  fs_kind: string | null;
  label: string | null;
  drive_letter: string | null;
  is_system: boolean;
}

export interface DiskInfo {
  id: string;
  display_name: string;
  size_bytes: number;
  free_bytes: number | null;
  model: string | null;
  serial: string | null;
  is_removable: boolean;
  is_system: boolean;
  bus_type: BusType;
  partitions: PartitionInfo[];
}

export type RecoveryStatusDto =
  | "Intact"
  | "PartiallyRecovered"
  | "Overwritten"
  | "Carved"
  | "CarvedValidated"
  | "PendingValidation";

export interface RecoveredFile {
  id: string;
  name: string;
  parent_path: string;
  size_bytes: number;
  mtime: string | null;
  status: RecoveryStatusDto;
  extension: string;
  mime: string | null;
  confidence: number;
  was_deleted: boolean;
}

export type ScanPhase =
  | "Probing"
  | "QuickScan"
  | "DeepScan"
  | "Carving"
  | "Finalizing"
  | "Done"
  | "Cancelled"
  | "Failed";

export interface ScanProgress {
  scan_id: string;
  phase: ScanPhase;
  current: u64_like;
  total: u64_like;
  message: string;
}
type u64_like = number;

export interface StartScanResponse {
  scan_id: string;
}

export interface SetScanPausedResponse {
  paused: boolean;
}

export interface ListResultsResponse {
  total: number;
  spilled: number;
  items: RecoveredFile[];
}

export interface CategoryCountDto {
  category: string;
  label: string;
  files: number;
  bytes: number;
}

export interface StatusCountDto {
  status: string;
  files: number;
  bytes: number;
}

export interface ScanStatsDto {
  scan_id: string;
  total_files: number;
  total_bytes: number;
  by_status: StatusCountDto[];
  by_category: CategoryCountDto[];
}

export interface FileRecoveryOutcome {
  entry_id: string;
  name: string;
  ok: boolean;
  path: string | null;
  bytes_written: number;
  partial: boolean;
  error: string | null;
}

export interface RecoveryProgress {
  scan_id: string;
  done: number;
  total: number;
  current_file: string;
  bytes_written: number;
}

export type CollisionPolicy = "keep_both" | "error" | "overwrite";

export interface RecoverFilesResponse {
  recovered: number;
  failed: number;
  bytes_written: number;
  results: FileRecoveryOutcome[];
  warnings: string[];
  errors: string[];
}

export interface SavedScanSummary {
  scan_id: string;
  source_uri: string;
  created_at: string;
  mode: "quick" | "deep";
  state: string;
  entries: number;
}

export interface ListPartitionsResponse {
  table_kind: "gpt" | "mbr" | "none";
  partitions: PartitionInfo[];
}

/** Dev hooks (CI screenshot tours + mock browser tours). Mirrors the
 * Rust `get_dev_hooks` command (env-var driven; harmless in prod). */
export interface DevHooks {
  tour: boolean;
  tour_mode: string | null;
  scan: string | null;
}
