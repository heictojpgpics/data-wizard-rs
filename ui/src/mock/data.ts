/**
 * Mock data generator (DEV/TEST ONLY): a synthetic recovery scenario —
 * realistic disks, a deleted-file population with believable names,
 * paths, sizes, timestamps and statuses. Shared by the browser mock
 * backend (mock/commands.ts); NEVER imported from product code.
 */

import type {
  DiskInfo,
  RecoveredFile,
  RecoveryStatusDto,
  SavedScanSummary,
} from "../bindings";

/* ── Deterministic PRNG (mulberry32) — same scenario every reload ─────── */

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const rand = mulberry32(0x5eed_1a7e);
const randInt = (min: number, max: number) => min + Math.floor(rand() * (max - min + 1));
const pick = <T,>(arr: readonly T[]): T => arr[Math.floor(rand() * arr.length)];

/* ── Disks ─────────────────────────────────────────────────────────────── */

const GB = 1024 ** 3;
const MB = 1024 ** 2;

export const MOCK_DISKS: DiskInfo[] = [
  {
    id: "\\\\.\\PhysicalDrive0",
    display_name: "Samsung SSD 990 PRO 1TB",
    size_bytes: 931 * GB,
    is_removable: false,
    is_system: true,
    bus_type: "Nvme",
    partitions: [
      { index: 0, offset_bytes: 1048576, size_bytes: 260 * MB, fs_kind: "fat32", label: "System EFI", drive_letter: null, is_system: true },
      { index: 1, offset_bytes: 273 * MB, size_bytes: 649 * MB, fs_kind: null, label: null, drive_letter: null, is_system: true },
      { index: 2, offset_bytes: 2 * GB - 500 * MB, size_bytes: 925 * GB, fs_kind: "ntfs", label: "Windows", drive_letter: "C:", is_system: true },
    ],
  },
  {
    id: "\\\\.\\PhysicalDrive1",
    display_name: "WD Elements 25A3 Media",
    size_bytes: 1.82 * 1024 * GB,
    is_removable: true,
    is_system: false,
    bus_type: "Usb",
    partitions: [
      { index: 0, offset_bytes: 1048576, size_bytes: 1.4 * 1024 * GB, fs_kind: "ntfs", label: "Backups", drive_letter: "D:", is_system: false },
      { index: 1, offset_bytes: 1.4 * 1024 * GB + 2 * MB, size_bytes: 447 * GB, fs_kind: "exfat", label: "Media", drive_letter: "E:", is_system: false },
    ],
  },
  {
    id: "\\\\.\\PhysicalDrive2",
    display_name: "SanDisk Ultra FIT 64GB",
    size_bytes: 59.6 * GB,
    is_removable: true,
    is_system: false,
    bus_type: "Usb",
    partitions: [
      { index: 0, offset_bytes: 1048576, size_bytes: 59.5 * GB, fs_kind: "fat32", label: "PHOTOS", drive_letter: "F:", is_system: false },
    ],
  },
  {
    id: "\\\\.\\PhysicalDrive3",
    display_name: "Kingston A400 480GB",
    size_bytes: 447 * GB,
    is_removable: false,
    is_system: false,
    bus_type: "Sata",
    partitions: [
      { index: 0, offset_bytes: 1048576, size_bytes: 445 * GB, fs_kind: "ext4", label: "linux-root", drive_letter: null, is_system: false },
    ],
  },
];

export function mockDiskByUri(uri: string): DiskInfo | undefined {
  return MOCK_DISKS.find((d) => d.id === uri);
}

export function mockPartitionOf(uri: string, index: number | null) {
  const disk = mockDiskByUri(uri);
  if (!disk) return null;
  if (index == null) return null;
  return disk.partitions.find((p) => p.index === index) ?? null;
}

export function mockSourceLabel(uri: string, partition: number | null): { label: string; detail: string } {
  const disk = mockDiskByUri(uri);
  if (disk) {
    if (partition != null) {
      const part = disk.partitions.find((p) => p.index === partition);
      if (part) {
        return {
          label: `${part.drive_letter ? `${part.drive_letter} ` : ""}${part.label ?? "Partition"}`,
          detail: `${part.fs_kind?.toUpperCase() ?? "Unknown FS"} · on ${disk.display_name}`,
        };
      }
    }
    return { label: disk.display_name, detail: "Whole disk" };
  }
  // A file image.
  const name = uri.split(/[\\/]/).pop() ?? uri;
  return { label: name, detail: "Disk image" };
}

/* ── The deleted-file population ──────────────────────────────────────── */

interface FileTemplate {
  dir: string;
  names: string[];
  exts: string[];
  min: number;
  max: number;
  weight: number;
  /** Mostly-deleted carve population (deep scan only)? */
  deep?: boolean;
}

const TEMPLATES: FileTemplate[] = [
  {
    dir: "Users\\anna\\Pictures\\Camera Roll",
    names: ["IMG_{n}", "DSC_{n}", "P{n}", "photo_{n}"],
    exts: ["jpg", "jpg", "jpg", "png", "heic"],
    min: 2 * MB, max: 9 * MB, weight: 26,
  },
  {
    dir: "Users\\anna\\Pictures\\2026 Spain",
    names: ["IMG_{n}", "IMG_{n}-edit", "spanish-coast-{n}"],
    exts: ["jpg", "jpg", "dng", "mp4"],
    min: 4 * MB, max: 48 * MB, weight: 12,
  },
  {
    dir: "Users\\anna\\Videos",
    names: ["vid_{n}", "holiday-{n}", "birthday-2025-{n}", "screen-rec-{n}"],
    exts: ["mp4", "mov", "mp4"],
    min: 120 * MB, max: 3.5 * 1024 * MB, weight: 6,
  },
  {
    dir: "Users\\anna\\Documents",
    names: ["report-2026-{n}", "invoice-{n}", "notes-{n}", "thesis-draft-{n}", "budget-{n}"],
    exts: ["docx", "pdf", "xlsx", "txt"],
    min: 40 * 1024, max: 18 * MB, weight: 14,
  },
  {
    dir: "Users\\anna\\Documents\\Work\\Contracts",
    names: ["contract-{n}", "nda-{n}", "agreement-{n}"],
    exts: ["pdf", "docx"],
    min: 200 * 1024, max: 6 * MB, weight: 6,
  },
  {
    dir: "Users\\anna\\Music",
    names: ["track_{n}", "live-set-{n}", "podcast-ep{n}"],
    exts: ["mp3", "flac", "m4a"],
    min: 3 * MB, max: 90 * MB, weight: 8,
  },
  {
    dir: "Users\\anna\\Downloads",
    names: ["setup-{n}", "ubuntu-26.04-{n}", "driver-pack-{n}", "ebook-{n}"],
    exts: ["exe", "iso", "zip", "epub"],
    min: 8 * MB, max: 4.7 * 1024 * MB, weight: 7,
  },
  {
    dir: "Users\\anna\\Archives",
    names: ["backup-{n}", "photos-2025-{n}", "old-laptop-{n}"],
    exts: ["zip", "7z", "tar"],
    min: 50 * MB, max: 6 * 1024 * MB, weight: 5,
  },
  {
    dir: "Users\\anna\\Projects\\website",
    names: ["index", "main-{n}", "styles", "api-{n}", "README"],
    exts: ["tsx", "ts", "css", "json", "md"],
    min: 2 * 1024, max: 340 * 1024, weight: 9,
  },
  {
    dir: "Users\\anna\\AppData\\Local\\Thunderbird\\Profiles\\u7x2k.default\\Mail",
    names: ["Inbox", "Sent", "Archive-{n}", "LocalFolders"],
    exts: ["eml", "mbox"],
    min: 4 * 1024, max: 80 * MB, weight: 3,
  },
  {
    dir: "Unallocated (carved)",
    names: ["carved_{n}", "frag_{n}", "recovered-{n}"],
    exts: ["jpg", "jpg", "png", "mp4", "pdf", "zip", "mp3"],
    min: 300 * 1024, max: 60 * MB, weight: 18, deep: true,
  },
  {
    dir: "Unallocated (carved)",
    names: ["deep_{n}", "raw_{n}"],
    exts: ["nef", "cr3", "dng", "mov"],
    min: 12 * MB, max: 80 * MB, weight: 8, deep: true,
  },
];

function formatTemplate(t: FileTemplate, i: number): string {
  const base = pick(t.names).replace("{n}", String(randInt(1000, 9999) + i));
  const ext = pick(t.exts);
  return `${base}.${ext}`;
}

function statusFor(isDeepFile: boolean, roll: number): RecoveryStatusDto {
  if (isDeepFile) {
    // Carved population: mostly Carved / CarvedValidated.
    if (roll < 0.55) return "CarvedValidated";
    if (roll < 0.85) return "Carved";
    if (roll < 0.95) return "PartiallyRecovered";
    return "PendingValidation";
  }
  // Metadata population: mostly intact.
  if (roll < 0.72) return "Intact";
  if (roll < 0.84) return "Intact";
  if (roll < 0.93) return "PartiallyRecovered";
  return "Overwritten";
}

function confidenceFor(s: RecoveryStatusDto): number {
  switch (s) {
    case "Intact": return randInt(92, 99);
    case "CarvedValidated": return randInt(78, 95);
    case "Carved": return randInt(45, 72);
    case "PartiallyRecovered": return randInt(30, 65);
    case "PendingValidation": return randInt(40, 60);
    case "Overwritten": return randInt(1, 12);
  }
}

/** An ISO-ish timestamp string like Rust's OffsetDateTime output. */
function mockFsTime(): string {
  // Random date within the last ~2.5 years.
  const now = Date.now() - randInt(0, 800) * 86400_000;
  const d = new Date(now);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())}.0 +00:00:00`;
}

export interface MockEntry extends RecoveredFile {
  /** Sort key: the order the engine "finds" this entry. */
  foundOrder: number;
  /** Deep-only population (quick scans skip it). */
  deepOnly: boolean;
}

function buildPopulation(): MockEntry[] {
  const out: MockEntry[] = [];
  let n = 0;
  for (const t of TEMPLATES) {
    for (let i = 0; i < t.weight; i++) {
      const isDeep = !!t.deep;
      const status = statusFor(isDeep, rand());
      const size = randInt(t.min, t.max);
      const ext = formatTemplate(t, i).split(".").pop() ?? "bin";
      out.push({
        id: `entry-${n}`,
        name: formatTemplate(t, i),
        parent_path: t.dir,
        size_bytes: size,
        mtime: mockFsTime(),
        status,
        extension: ext,
        mime: null,
        confidence: confidenceFor(status),
        was_deleted: true,
        foundOrder: n,
        deepOnly: isDeep,
      });
      n++;
    }
  }
  return out;
}

export const MOCK_FILES: MockEntry[] = buildPopulation();
export const QUICK_FILES = MOCK_FILES.filter((f) => !f.deepOnly);
export const DEEP_FILES = MOCK_FILES;

/* ── Saved scans (pre-seeded) ─────────────────────────────────────────── */

function hoursAgo(h: number): string {
  const d = new Date(Date.now() - h * 3600_000);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())}.0 +00:00:00`;
}

export const MOCK_SAVED_SCANS: SavedScanSummary[] = [
  {
    scan_id: "saved-3f9c2e71",
    source_uri: "\\\\.\\PhysicalDrive1",
    created_at: hoursAgo(2.4),
    mode: "deep",
    state: "complete",
    entries: DEEP_FILES.length,
  },
  {
    scan_id: "saved-a0d81b52",
    source_uri: "C:\\Users\\anna\\Downloads\\sdcard.img",
    created_at: hoursAgo(29),
    mode: "quick",
    state: "complete",
    entries: QUICK_FILES.length,
  },
];
