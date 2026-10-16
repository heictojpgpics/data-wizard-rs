/**
 * Scan lifecycle store — start/resume/cancel a scan, track live
 * progress (the scan:progress event), and feed "found" rows to the
 * live feed while the engine streams them in.
 *
 * The store keeps ONE persistent event listener installed at boot so
 * the scan lifecycle survives every view switch (the lesson from
 * DiskGenie's session-5 fix — a per-mount listener drops progress
 * that arrives while a view is between mounts).
 */
import { create } from "zustand";
import { invoke, listen } from "../lib/ipc";
import type { RecoveredFile, ScanProgress } from "../bindings";
import { useAppStore } from "./app";

export interface ScanState {
  progress: ScanProgress | null;
  /** Files/min measured client-side from the current counter. */
  rate: number | null;
  /** Whether the persistent listeners are installed. */
  listenersReady: boolean;
  ensureListeners: () => void;

  startScan: (uri: string, deep: boolean, partition: number | null) => Promise<string>;
  resumeScan: (savedScanId: string, deep: boolean) => Promise<string>;
  cancelScan: () => Promise<void>;
  /** Poll the scan's terminal state (Done/Failed/Cancelled). */
  awaitCompletion: () => Promise<void>;
}

let firstCount: { count: number; at: number } | null = null;

export const useScanStore = create<ScanState>((set, get) => ({
  progress: null,
  rate: null,
  listenersReady: false,

  ensureListeners: () => {
    if (get().listenersReady) return;
    set({ listenersReady: true });

    void listen<ScanProgress>("scan:progress", (p) => {
      const app = useAppStore.getState();
      if (app.scanId && p.scan_id !== app.scanId) return;

      set({ progress: p });

      // Rate: measured from the current counter between >=1s spans.
      const now = Date.now();
      if (p.current > 0 && p.phase === "QuickScan") {
        if (firstCount === null) {
          firstCount = { count: p.current, at: now };
        } else if (p.current > firstCount.count) {
          const dt = (now - firstCount.at) / 1000;
          if (dt >= 1) {
            set({ rate: (p.current - firstCount.count) / dt });
          }
        }
      }

      // Terminal states clear the measuring window.
      if (p.phase === "Done" || p.phase === "Failed" || p.phase === "Cancelled") {
        firstCount = null;
      }
    });

    // Some engines stream found rows through the progress path as
    // list_results snapshots — the live feed is enriched by polling
    // during QuickScan (see ScanningView). Nothing else needed here.
  },

  startScan: async (uri, deep, partition) => {
    const res = await invoke<{ scan_id: string }>("start_scan", {
      req: {
        source_uri: uri,
        deep,
        max_results: 1_000_000,
        file_filter: [],
        partition: partition ?? undefined,
      },
    });
    set({ progress: null, rate: null });
    return res.scan_id;
  },

  resumeScan: async (savedScanId, deep) => {
    const res = await invoke<{ scan_id: string }>("resume_scan", {
      req: { saved_scan_id: savedScanId, deep },
    });
    set({ progress: null, rate: null });
    return res.scan_id;
  },

  cancelScan: async () => {
    const scanId = useAppStore.getState().scanId;
    if (!scanId) return;
    await invoke("cancel_scan", { req: { scan_id: scanId } }).catch(() => undefined);
  },

  awaitCompletion: async () => {
    const poll = (resolve: () => void) => {
      const p = get().progress;
      if (p && (p.phase === "Done" || p.phase === "Failed" || p.phase === "Cancelled")) {
        resolve();
        return;
      }
      window.setTimeout(() => poll(resolve), 300);
    };
    return new Promise<void>((resolve) => poll(resolve));
  },
}));

/** Convenience: fetch one page of results (server-side filter aware). */
export async function fetchResultsPage(
  scanId: string,
  offset: number,
  limit: number,
  extensionFilter: string[] | null,
): Promise<{ items: RecoveredFile[]; total: number; spilled: number }> {
  const res = await invoke<{ items: RecoveredFile[]; total: number; spilled: number }>(
    "list_results",
    {
      req: {
        scan_id: scanId,
        offset,
        limit,
        extension_filter: extensionFilter,
      },
    },
  );
  return { items: res.items ?? [], total: res.total ?? 0, spilled: res.spilled ?? 0 };
}
