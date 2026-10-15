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
  fs_kind: string | null;
  label: string | null;
  drive_letter: string | null;
  is_system: boolean;
}

export interface DiskInfo {
  id: string;
  display_name: string;
  size_bytes: number;
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
  current: number;
  total: number;
  message: string;
}

export interface StartScanResponse {
  scan_id: string;
}

export interface ListResultsResponse {
  total: number;
  spilled: number;
  items: RecoveredFile[];
}

export interface AgeBucketDto {
  month: string;
  files: number;
  bytes: number;
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
  by_month: AgeBucketDto[];
  no_timestamp_files: number;
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
