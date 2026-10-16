// Formatting, classification and display helpers shared across views.

import type { RecoveryStatusDto } from "../bindings";

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  if (n < 1024 ** 4) return `${(n / 1024 ** 3).toFixed(2)} GB`;
  return `${(n / 1024 ** 4).toFixed(2)} TB`;
}

/** Compact integer with thousands separators. */
export function formatCount(n: number): string {
  return n.toLocaleString(undefined);
}

/** Rate like "1.2k files/min" / "834 files/min". */
export function formatRate(perSecond: number): string {
  const perMin = perSecond * 60;
  if (perMin >= 1000) return `${(perMin / 1000).toFixed(1)}k files/min`;
  return `${Math.round(perMin)} files/min`;
}

/**
 * Parse the string form produced by Rust's `OffsetDateTime::to_string`
 * ("2025-03-01 12:00:00.0 +00:00:00") into a Date. Returns null when the
 * value is absent or unparseable.
 */
export function parseFsTime(s: string | null): Date | null {
  if (!s) return null;
  // "YYYY-MM-DD HH:MM:SS.f +OO:OO:OO" -> "YYYY-MM-DDTHH:MM:SS+OO:OO"
  const m = s.match(
    /^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})(?:\.\d+)?\s*([+-]\d{2}:\d{2})?/,
  );
  if (m) {
    const iso = `${m[1]}T${m[2]}${m[3] ?? "Z"}`;
    const d = new Date(iso);
    if (!Number.isNaN(d.getTime())) return d;
  }
  const fallback = new Date(s);
  return Number.isNaN(fallback.getTime()) ? null : fallback;
}

export function formatDate(d: Date | null): string {
  if (!d) return "unknown";
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function formatDateTime(d: Date | null): string {
  if (!d) return "unknown";
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Human label for a recovery status. */
export function statusLabel(s: RecoveryStatusDto): string {
  switch (s) {
    case "Intact":
      return "Intact";
    case "PartiallyRecovered":
      return "Partial";
    case "Overwritten":
      return "Overwritten";
    case "Carved":
      return "Carved";
    case "CarvedValidated":
      return "Carved, verified";
    case "PendingValidation":
      return "Unchecked";
  }
}

/** One-line plain-language explanation of what a status means. */
export function statusExplanation(s: RecoveryStatusDto): string {
  switch (s) {
    case "Intact":
      return "File records are complete and the data blocks are untouched — this should recover exactly as it was.";
    case "PartiallyRecovered":
      return "Some data blocks are missing or damaged; the file will recover but may be incomplete or corrupt.";
    case "Overwritten":
      return "New data has taken this file's place on disk. Recovery is not possible for this entry.";
    case "Carved":
      return "Found by raw signature scan in unallocated space. Size is estimated from the header.";
    case "CarvedValidated":
      return "Found by raw scan and structure-checked (e.g. JPEG end-marker verified) — high confidence.";
    case "PendingValidation":
      return "Found by raw scan; structure validation has not run for this type yet.";
  }
}

/** CSS var for a status color (tokens define per-theme values). */
export function statusVar(s: RecoveryStatusDto): string {
  switch (s) {
    case "Intact":
      return "var(--st-intact)";
    case "PartiallyRecovered":
      return "var(--st-partial)";
    case "Overwritten":
      return "var(--st-overwritten)";
    case "Carved":
      return "var(--st-carved)";
    case "CarvedValidated":
      return "var(--st-validated)";
    case "PendingValidation":
      return "var(--st-pending)";
  }
}

/* ═════════════════════════════════════════════════════════════════════
 * Category system — a TS mirror of dw-core's classify.rs so the UI can
 * bucket rows and drive the category rail without extra IPC.
 * ═════════════════════════════════════════════════════════════════════ */

export type CategoryId =
  | "photos"
  | "videos"
  | "audio"
  | "documents"
  | "archives"
  | "email"
  | "code"
  | "other";

export const CATEGORIES: { id: CategoryId; label: string; varName: string }[] = [
  { id: "photos", label: "Photos", varName: "--cat-photos" },
  { id: "videos", label: "Videos", varName: "--cat-videos" },
  { id: "audio", label: "Audio", varName: "--cat-audio" },
  { id: "documents", label: "Documents", varName: "--cat-documents" },
  { id: "archives", label: "Archives", varName: "--cat-archives" },
  { id: "email", label: "Email", varName: "--cat-email" },
  { id: "code", label: "Code", varName: "--cat-code" },
  { id: "other", label: "Other", varName: "--cat-other" },
];

/** CSS var for a category's color. */
export function categoryVar(c: CategoryId): string {
  const entry = CATEGORIES.find((x) => x.id === c);
  return `var(${entry?.varName ?? "--cat-other"})`;
}

const EXT_TO_CATEGORY: Record<string, CategoryId> = {};
function reg(category: CategoryId, exts: string[]): void {
  for (const e of exts) EXT_TO_CATEGORY[e] = category;
}
reg("photos", "jpg jpeg jpe jfif png gif bmp tif tiff webp heic heif avif ico psd svg dng cr2 cr3 nef arw orf rw2 raf raw".split(" "));
reg("videos", "mp4 m4v mov avi mkv webm wmv flv f4v mpg mpeg mp2 3gp 3g2 mts m2ts mxf vob ogv".split(" "));
reg("audio", "mp3 m4a aac wav flac ogg oga opus wma aiff aif mid midi amr".split(" "));
reg("documents", "pdf doc docx xls xlsx ppt pptx odt ods odp rtf txt md csv tsv epub pages numbers key hwp".split(" "));
reg("archives", "zip rar 7z tar gz bz2 xz zst lz4 tgz tbz2 txz cab iso dmg vhd vhdx wim img dd vmdk".split(" "));
reg("email", "pst ost eml msg mbox dbx".split(" "));
reg("code", "rs c h cpp hpp cc py js jsx ts tsx java kt go rb php cs swift m mm sh ps1 bat cmd html htm css scss json xml yml yaml toml ini cfg conf sql ipynb".split(" "));

/** Classify a file extension (lowercase, no dot) — mirrors the Rust
 * classifier's buckets. */
export function categoryOf(ext: string): CategoryId {
  return EXT_TO_CATEGORY[ext] ?? "other";
}

/** The extension list for one category (used to build server-side
 * extension_filter requests for list_results). */
export function extensionsOf(c: CategoryId): string[] {
  return Object.entries(EXT_TO_CATEGORY)
    .filter(([, cat]) => cat === c)
    .map(([ext]) => ext);
}

/* ── File-type glyph metadata (per-extension accent + label) ─────────── */

export function fileKindLabel(ext: string): string {
  const c = categoryOf(ext);
  return CATEGORIES.find((x) => x.id === c)?.label ?? "Other";
}

/** Bus type display label. */
export function busLabel(bus: string): string {
  switch (bus) {
    case "Nvme":
      return "NVMe SSD";
    case "Sata":
      return "SATA";
    case "Usb":
      return "USB";
    case "Sd":
      return "SD card";
    case "Mmc":
      return "MMC";
    case "Virtual":
      return "Virtual";
    case "IScsi":
      return "iSCSI";
    case "FibreChannel":
      return "Fibre Channel";
    default:
      return "Disk";
  }
}

/** Short relative-ish duration ("3 min ago" style) for saved scans. */
export function timeAgo(iso: string): string {
  const t = parseFsTime(iso) ?? new Date(iso);
  const secs = Math.max(0, (Date.now() - t.getTime()) / 1000);
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.floor(secs / 60)} min ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)} h ago`;
  return `${Math.floor(secs / 86400)} d ago`;
}
