// Formatting and small display helpers shared across views.

import type { RecoveryStatusDto } from "../bindings";

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
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
    /^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})(?:\.\d+)?\s*([+-]\d{2}:\d{2})?/
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

/** Tailwind text color for a status. */
export function statusColor(s: RecoveryStatusDto): string {
  switch (s) {
    case "Intact":
      return "text-emerald-400";
    case "PartiallyRecovered":
      return "text-amber-400";
    case "Overwritten":
      return "text-red-400";
    case "Carved":
      return "text-sky-400";
    case "CarvedValidated":
      return "text-cyan-300";
    case "PendingValidation":
      return "text-zinc-400";
  }
}

/** Tailwind chip background for a status. */
export function statusChip(s: RecoveryStatusDto): string {
  switch (s) {
    case "Intact":
      return "bg-emerald-500/10 text-emerald-400 border-emerald-500/30";
    case "PartiallyRecovered":
      return "bg-amber-500/10 text-amber-400 border-amber-500/30";
    case "Overwritten":
      return "bg-red-500/10 text-red-400 border-red-500/30";
    case "Carved":
      return "bg-sky-500/10 text-sky-400 border-sky-500/30";
    case "CarvedValidated":
      return "bg-cyan-500/10 text-cyan-300 border-cyan-500/30";
    case "PendingValidation":
      return "bg-zinc-500/10 text-zinc-400 border-zinc-500/30";
  }
}

export function categoryColor(index: number): string {
  const palette = [
    "#38bdf8",
    "#818cf8",
    "#f472b6",
    "#34d399",
    "#fbbf24",
    "#f87171",
    "#a78bfa",
    "#94a3b8",
  ];
  return palette[index % palette.length];
}
