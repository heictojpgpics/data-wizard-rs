/**
 * Recovery store — the destination validation, the live session and
 * the outcome summary. Wired to the `recovery:progress` event channel
 * (per-file stream) and the recover_files command.
 *
 * EaseUS-parity behaviors:
 *  - destination rows with capacity + free space
 *  - source-drive protection (block + explain, never silently allow)
 *  - free-space preflight ("Need additional %1 of space")
 *  - live per-file stream + counters during recovery
 *  - failure list with reasons at completion
 */

import { create } from "zustand";
import { invoke, listen } from "../lib/ipc";
import type { CollisionPolicy, FileRecoveryOutcome } from "../bindings";
import { useAppStore } from "./app";
import { useResultsStore, type RowMeta } from "./results";
import { useUiStore } from "./ui";

export type RecoveryStage = "closed" | "destination" | "running" | "summary";

export interface RecoveryOutcome {
  ok: boolean;
  file: FileRecoveryOutcome;
}

interface RecoveryStore {
  stage: RecoveryStage;

  /* destination picking */
  destination: string | null;
  destinationLabel: string | null;
  collision: CollisionPolicy;

  /* session */
  scanId: string | null;
  done: number;
  total: number;
  currentFile: string;
  bytesWritten: number;
  cancelRequested: boolean;

  /* summary */
  outcomes: RecoveryOutcome[];
  warnings: string[];
  errors: string[];
  summaryAt: number;

  listenersReady: boolean;
  ensureListeners: () => void;

  open: (files: RowMeta[] | "selected") => void;
  close: () => void;
  setDestination: (dir: string, label: string) => void;
  setCollision: (p: CollisionPolicy) => void;
  run: () => Promise<void>;
  requestCancel: () => void;
}

export const useRecoveryStore = create<RecoveryStore>((set, get) => ({
  stage: "closed",

  destination: null,
  destinationLabel: null,
  collision: "keep_both",

  scanId: null,
  done: 0,
  total: 0,
  currentFile: "",
  bytesWritten: 0,
  cancelRequested: false,

  outcomes: [],
  warnings: [],
  errors: [],
  summaryAt: 0,

  listenersReady: false,

  ensureListeners: () => {
    if (get().listenersReady) return;
    set({ listenersReady: true });

    void listen<{
      scan_id: string;
      done: number;
      total: number;
      current_file: string;
      bytes_written: number;
    }>("recovery:progress", (p) => {
      const st = get();
      if (st.stage !== "running" || (st.scanId && p.scan_id !== st.scanId)) return;
      set({
        done: p.done,
        total: p.total,
        currentFile: p.current_file,
        bytesWritten: p.bytes_written,
      });
    });
  },

  open: (files) => {
    const app = useAppStore.getState();
    if (!app.scanId) return;
    const results = useResultsStore.getState();
    const sel =
      files === "selected"
        ? [...results.selected].map((id) => results.byId.get(id)).filter((m): m is RowMeta => m != null)
        : files;
    if (sel.length === 0) {
      useUiStore.getState().pushToast({
        kind: "info",
        title: "Nothing selected",
        body: "Pick at least one file to recover.",
      });
      return;
    }
    set({
      stage: "destination",
      scanId: app.scanId,
      destination: null,
      destinationLabel: null,
      collision: "keep_both",
      done: 0,
      total: sel.length,
      currentFile: "",
      bytesWritten: 0,
      cancelRequested: false,
      outcomes: [],
      warnings: [],
      errors: [],
    });
    get().ensureListeners();
  },

  close: () => set({ stage: "closed" }),

  setDestination: (dir, label) => set({ destination: dir, destinationLabel: label }),
  setCollision: (p) => set({ collision: p }),

  run: async () => {
    const st = get();
    const app = useAppStore.getState();
    if (!st.destination || !st.scanId || !app.scanId) return;

    const results = useResultsStore.getState();
    const ids =
      st.stage === "destination"
        ? [...results.selected]
        : [...results.selected];

    set({ stage: "running", done: 0, bytesWritten: 0, currentFile: "", cancelRequested: false });

    try {
      const res = await invoke<{
        recovered: number;
        failed: number;
        bytes_written: number;
        results: FileRecoveryOutcome[];
        warnings: string[];
        errors: string[];
      }>("recover_files", {
        req: {
          scan_id: st.scanId,
          entry_ids: ids,
          destination_dir: st.destination,
          collision_policy: st.collision,
        },
      });
      set({
        stage: "summary",
        outcomes: (res.results ?? []).map((file) => ({ ok: file.ok, file })),
        warnings: res.warnings ?? [],
        errors: res.errors ?? [],
        summaryAt: Date.now(),
        done: (res.results ?? []).length,
        total: (res.results ?? []).length,
      });
    } catch (e) {
      useUiStore.getState().pushToast({
        kind: "err",
        title: "Recovery failed",
        body: String(e),
      });
      set({ stage: "destination" });
    }
  },

  requestCancel: () => set({ cancelRequested: true }),
}));

/* ── Summary helpers ─────────────────────────────────────────────────── */

export function recoverySummary(outcomes: RecoveryOutcome[]): {
  ok: number;
  failed: number;
  bytes: number;
  failedList: FileRecoveryOutcome[];
} {
  let ok = 0;
  let bytes = 0;
  const failedList: FileRecoveryOutcome[] = [];
  for (const o of outcomes) {
    if (o.ok) {
      ok++;
      bytes += o.file.bytes_written;
    } else {
      failedList.push(o.file);
    }
  }
  return { ok, failed: failedList.length, bytes, failedList };
}

/** The failure-reason hints (EaseUS's numbered advice list). */
export const FAILURE_REASONS = [
  "Insufficient storage space at the destination.",
  "The recovery path was too long for the filesystem.",
  "The file name contained characters the destination rejects.",
  "The source sectors could not be re-read (failing disk).",
];

/** Guard: warn loudly when the user picks the SOURCE drive. */
export function isSourceDrive(dest: string, sourceUri: string | null): boolean {
  if (!sourceUri) return false;
  // Compare the leading drive letter / device path, case-insensitive.
  const norm = (s: string) => s.replace(/\\/g, "/").toLowerCase();
  const d = norm(dest);
  const s = norm(sourceUri);
  const driveOf = (x: string) => {
    const m = x.match(/^\/?\*?volume\{([^}]+)\}/) ?? x.match(/^([a-z]:)/);
    return m ? m[1] : null;
  };
  const dd = driveOf(d);
  const sd = driveOf(s);
  return dd != null && dd === sd;
}
