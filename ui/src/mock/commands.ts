/**
 * Mock command registry (DEV/TEST ONLY): implements the full Data
 * Wizard Tauri command surface against the synthetic scenario so the
 * UI runs in a plain browser. Used by Playwright/agent-browser
 * screenshot passes and the ?tour=1 browser tour; NEVER bundled into
 * production (gated on isTauri()=false + DEV in main.tsx).
 *
 * Scan lifecycle mirrors the Rust engine: start_scan spawns a ticker
 * that emits `scan:progress` through the phases (Probing -> QuickScan
 * [-> DeepScan -> Carving] -> Finalizing -> Done); list_results grows
 * live during the run; cancel_scan stops the ticker mid-flight.
 */
import { emitMockEvent, setMockBackend } from "../lib/ipc";
import type {
  DevHooks,
  ListPartitionsResponse,
  RecoverFilesResponse,
  RecoveredFile,
  ScanProgress,
  ScanStatsDto,
} from "../bindings";
import {
  DEEP_FILES,
  MOCK_DISKS,
  MOCK_FILES,
  MOCK_SAVED_SCANS,
  QUICK_FILES,
  mockDiskByUri,
  type MockEntry,
} from "./data";
import { extensionsOf, type CategoryId } from "../lib/format";

type Req = Record<string, unknown> | undefined;

const req = <T,>(args: Req): T => (args?.req ?? {}) as T;

interface ScanRun {
  id: string;
  deep: boolean;
  files: MockEntry[];
  /** How many entries have "been found" so far. */
  found: number;
  phase: ScanProgress["phase"];
  ticker: number | null;
  cancelled: boolean;
  /** The pause gate (mirrors the Rust PausableSource). */
  paused: boolean;
}

let run: ScanRun | null = null;
let runCounter = 0;
/** Simulated wall clock for created_at fields. */
const nowIso = () => {
  const d = new Date();
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())}.0 +00:00:00`;
};

function currentResults(): MockEntry[] {
  if (!run) return [];
  return run.files.slice(0, run.found);
}

/* ── Category stats (mirrors dw-core classify) ─────────────────────────── */

import { categoryOf } from "../lib/format";

function computeStats(scanId: string): ScanStatsDto {
  const results = currentResults();
  const byCategory = new Map<string, { files: number; bytes: number }>();
  const byStatus = new Map<string, { files: number; bytes: number }>();
  let totalFiles = 0;
  let totalBytes = 0;
  for (const f of results) {
    totalFiles += 1;
    totalBytes += f.size_bytes;
    const cat = categoryOf(f.extension);
    const c = byCategory.get(cat) ?? { files: 0, bytes: 0 };
    c.files += 1;
    c.bytes += f.size_bytes;
    byCategory.set(cat, c);
    const s = byStatus.get(f.status) ?? { files: 0, bytes: 0 };
    s.files += 1;
    s.bytes += f.size_bytes;
    byStatus.set(f.status, s);
  }
  const catLabels: Record<string, string> = {
    photos: "Photos", videos: "Videos", audio: "Audio", documents: "Documents",
    archives: "Archives", email: "Email", code: "Code", other: "Other",
  };
  return {
    scan_id: scanId,
    total_files: totalFiles,
    total_bytes: totalBytes,
    by_status: [...byStatus.entries()].map(([status, v]) => ({ status, ...v })),
    by_category: [...byCategory.entries()].map(([category, v]) => ({
      category,
      label: catLabels[category] ?? category,
      ...v,
    })),
  };
}

/* ── The scan ticker ───────────────────────────────────────────────────── */

function emitProgress(p: Partial<ScanProgress> & { phase: ScanProgress["phase"] }) {
  if (!run) return;
  const payload: ScanProgress = {
    scan_id: run.id,
    phase: p.phase,
    current: p.current ?? run.found,
    total: p.total ?? run.files.length,
    message: p.message ?? "",
  };
  emitMockEvent("scan:progress", payload);
}

/** Advance the run one tick (~120ms of engine time). */
function tick(): void {
  if (!run || run.cancelled) return;
  if (run.paused) return;
  const files = run.files;

  switch (run.phase) {
    case "Probing": {
      emitProgress({ phase: "Probing", current: 0, message: "Reading volume boot record…" });
      run.phase = "QuickScan";
      return;
    }
    case "QuickScan": {
      if (run.found >= files.filter((f) => !f.deepOnly).length) {
        if (run.deep) {
          run.phase = "DeepScan";
          emitProgress({ phase: "DeepScan", current: 0, message: "Scanning unallocated clusters…" });
        } else {
          run.phase = "Finalizing";
        }
        return;
      }
      // Stream in 1-3% of the quick population per tick.
      const step = Math.max(2, Math.round(files.filter((f) => !f.deepOnly).length * 0.022));
      const target = Math.min(run.found + step, files.filter((f) => !f.deepOnly).length);
      run.found = target;
      emitProgress({ phase: "QuickScan", message: "Reading file records (MFT)…" });
      return;
    }
    case "DeepScan": {
      if (run.found >= files.length) {
        run.phase = "Carving";
        emitProgress({ phase: "Carving", current: 0, message: "Validating carved structures…" });
        return;
      }
      const step = Math.max(1, Math.round(files.length * 0.014));
      run.found = Math.min(run.found + step, files.length);
      emitProgress({ phase: "DeepScan", message: "Carving unallocated space (signature scan)…" });
      return;
    }
    case "Carving": {
      run.phase = "Finalizing";
      emitProgress({ phase: "Finalizing", message: "Building the results index…" });
      return;
    }
    case "Finalizing": {
      run.phase = "Done";
      emitProgress({ phase: "Done", current: run.found, message: "" });
      if (run.ticker !== null) window.clearInterval(run.ticker);
      run.ticker = null;
      return;
    }
    default:
      return;
  }
}

function startRun(deep: boolean): string {
  if (run?.ticker !== null && run?.ticker !== undefined) {
    window.clearInterval(run.ticker);
  }
  runCounter += 1;
  run = {
    id: `scan-${runCounter}-${Date.now().toString(36)}`,
    deep,
    files: deep ? DEEP_FILES : QUICK_FILES,
    found: 0,
    phase: "Probing",
    ticker: null,
    cancelled: false,
    paused: false,
  };
  // Fast enough to feel real, slow enough to see the phases (~2-4s quick).
  run.ticker = window.setInterval(tick, 110);
  tick();
  return run.id;
}

/* ── Recovery simulation ───────────────────────────────────────────────── */

interface RecoveryRun {
  cancelled: boolean;
  ticker: number | null;
}
let recoveryRun: RecoveryRun | null = null;

async function simulateRecovery(
  scanId: string,
  entries: RecoveredFile[],
  destination: string,
  policy: string,
): Promise<RecoverFilesResponse> {
  recoveryRun = { cancelled: false, ticker: null };
  const rr = recoveryRun;
  const results: RecoverFilesResponse["results"] = [];
  let bytesWritten = 0;

  for (let i = 0; i < entries.length; i++) {
    if (rr.cancelled) break;
    const e = entries[i];
    // Per-file dwell scaled by size (fast for the tour).
    await new Promise((r) => window.setTimeout(r, 70 + Math.min(220, e.size_bytes / 900_000)));
    const bad = e.status === "Overwritten";
    const partial = e.status === "PartiallyRecovered";
    const ok = !bad;
    const written = bad ? 0 : partial ? Math.floor(e.size_bytes * (0.3 + Math.random() * 0.5)) : e.size_bytes;
    bytesWritten += written;
    let path: string | null = null;
    if (ok) {
      // Collision rename under keep_both, the second time around.
      const suffix = policy === "keep_both" && i > 0 && Math.random() < 0.18 ? " (2)" : "";
      const dot = e.name.lastIndexOf(".");
      const renamed =
        dot > 0
          ? `${e.name.slice(0, dot)}${suffix}.${e.extension}`
          : `${e.name}${suffix}`;
      path = `${destination}\\${renamed}`;
    }
    results.push({
      entry_id: e.id,
      name: e.name,
      ok,
      path,
      bytes_written: written,
      partial,
      error: bad ? "Data blocks were overwritten on the source" : null,
    });
    emitMockEvent("recovery:progress", {
      scan_id: scanId,
      done: i + 1,
      total: entries.length,
      current_file: e.name,
      bytes_written: bytesWritten,
    });
  }

  const recovered = results.filter((r) => r.ok).length;
  const warnings = results.filter((r) => r.partial).map((r) => `${r.name} recovered partially — content may be incomplete.`);
  const errors = results.filter((r) => !r.ok).map((r) => `${r.name}: ${r.error}`);
  return {
    recovered,
    failed: results.length - recovered,
    bytes_written: bytesWritten,
    results,
    warnings: warnings.slice(0, 8),
    errors: errors.slice(0, 8),
  };
}

/* ── Command handlers ──────────────────────────────────────────────────── */

const handlers: Record<string, (args: Req) => Promise<unknown>> = {
  /* Sources ──────────────────────────────────────────────────────────── */
  async list_disks() {
    await delay(420);
    return { disks: MOCK_DISKS };
  },

  async list_partitions(args) {
    const { source_uri } = req<{ source_uri: string }>(args);
    const disk = mockDiskByUri(source_uri);
    await delay(260);
    if (!disk) {
      // A file image: GPT-flavored view of the sdcard scenario.
      if (source_uri.endsWith(".img") || source_uri.endsWith(".dd") || source_uri.endsWith(".raw")) {
        return {
          table_kind: "mbr",
          partitions: [
            { index: 0, offset_bytes: 4096 * 512, size_bytes: 32 * 1024 * 1024 * 1024, free_bytes: 11 * 1024 * 1024 * 1024, fs_kind: "fat32", label: "SD CARD", drive_letter: null, is_system: false },
          ],
        } satisfies ListPartitionsResponse;
      }
      throw new Error(`cannot open source: ${source_uri}`);
    }
    return {
      table_kind: "gpt",
      partitions: disk.partitions,
    } satisfies ListPartitionsResponse;
  },

  /* Scan lifecycle ───────────────────────────────────────────────────── */
  async start_scan(args) {
    const r = req<{ source_uri: string; deep: boolean }>(args);
    await delay(180);
    return { scan_id: startRun(r.deep) };
  },

  async resume_scan(args) {
    const r = req<{ saved_scan_id: string; deep: boolean }>(args);
    await delay(200);
    return { scan_id: startRun(r.deep) };
  },

  async cancel_scan() {
    if (run && run.phase !== "Done") {
      run.cancelled = true;
      if (run.ticker !== null) window.clearInterval(run.ticker);
      run.ticker = null;
      run.phase = "Cancelled";
      emitProgress({ phase: "Cancelled" });
    }
    return { cancelled: true };
  },

  async set_scan_paused(args) {
    const { paused } = req<{ scan_id: string; paused: boolean }>(args);
    if (run) {
      run.paused = paused;
    }
    return { paused: !!paused };
  },

  async get_scan_progress() {
    if (!run) throw new Error("no active scan");
    return {
      progress: {
        scan_id: run.id,
        phase: run.phase,
        current: run.found,
        total: run.files.length,
        message: "",
      },
      paused: run.paused,
    };
  },

  /* Results ──────────────────────────────────────────────────────────── */
  async list_results(args) {
    const r = req<{
      scan_id: string;
      offset: number;
      limit: number;
      extension_filter: string[] | null;
    }>(args);
    let items = currentResults();
    if (r.extension_filter && r.extension_filter.length > 0) {
      const set = new Set(r.extension_filter);
      items = items.filter((f) => set.has(f.extension));
    }
    const page = items.slice(r.offset, r.offset + r.limit);
    return {
      total: items.length,
      spilled: Math.max(0, items.length - 600),
      items: page,
    };
  },

  async scan_stats() {
    const id = run?.id ?? "scan-0";
    return { stats: computeStats(id) };
  },

  /* Saved scans ──────────────────────────────────────────────────────── */
  async list_saved_scans() {
    await delay(200);
    return { scans: MOCK_SAVED_SCANS };
  },

  async save_scan() {
    await delay(300);
    const entries = currentResults().length;
    if (run) {
      MOCK_SAVED_SCANS.unshift({
        scan_id: `saved-${Date.now().toString(36)}`,
        source_uri: "\\\\.\\PhysicalDrive1",
        created_at: nowIso(),
        mode: run.deep ? "deep" : "quick",
        state: "complete",
        entries,
      });
    }
    return { saved: true, entries };
  },

  async delete_saved_scan(args) {
    const { scan_id } = req<{ scan_id: string }>(args);
    const i = MOCK_SAVED_SCANS.findIndex((s) => s.scan_id === scan_id);
    if (i >= 0) MOCK_SAVED_SCANS.splice(i, 1);
    return { deleted: i >= 0 };
  },

  /* Recovery ─────────────────────────────────────────────────────────── */
  async recover_files(args) {
    const r = req<{
      scan_id: string;
      entry_ids: string[];
      destination_dir: string;
      collision_policy: string;
    }>(args);
    const results = currentResults();
    const entries = r.entry_ids
      .map((id) => results.find((f) => f.id === id))
      .filter((f): f is MockEntry => f != null);
    if (entries.length === 0) throw new Error("no matching entries for this scan");
    return simulateRecovery(r.scan_id, entries, r.destination_dir, r.collision_policy);
  },

  /* Dev hooks (tour mode; mirrors the Rust env-var command) ──────────── */
  async get_dev_hooks(): Promise<DevHooks> {
    // Browser mode: ?tour=1 drives the same program the Windows runner
    // captures through the real app.
    const params = new URLSearchParams(window.location.search);
    const tour = params.get("tour") === "1";
    return { tour, tour_mode: params.get("tourMode"), scan: null };
  },
};

function delay(ms: number): Promise<void> {
  return new Promise((r) => window.setTimeout(r, ms));
}

/** Install the mock backend (called from main.tsx in DEV browser mode). */
export function installMock(): void {
  setMockBackend((cmd, args) => {
    const h = handlers[cmd];
    if (!h) return Promise.reject(new Error(`mock: unknown command ${cmd}`));
    return h(args);
  });
  // Expose a small debug handle for agent-driven validation.
  (window as unknown as Record<string, unknown>).__DW_MOCK__ = {
    disks: MOCK_DISKS,
    files: MOCK_FILES,
    get run() {
      return run;
    },
    /** Fast-forward the scan to done (agent shortcut). */
    finishScan(): void {
      if (!run) return;
      run.found = run.files.length;
      run.phase = "Finalizing";
      tick();
    },
    /** Category extension lists (parity check helper). */
    extensionsOf: (c: CategoryId) => extensionsOf(c),
  };

  // Agent-debug channel: live store snapshots (DEV/browser only).
  void Promise.all([
    import("../state/app"),
    import("../state/scan"),
    import("../state/results"),
    import("../state/devices"),
    import("../state/recovery"),
  ]).then(([app, scan, results, devices, recovery]) => {
    (window as unknown as Record<string, unknown>).__DW_STORES__ = {
      app: app.useAppStore,
      scan: scan.useScanStore,
      results: results.useResultsStore,
      devices: devices.useDevicesStore,
      recovery: recovery.useRecoveryStore,
    };
  });
}
