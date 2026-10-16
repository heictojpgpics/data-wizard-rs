/**
 * Devices store — the disk/partition inventory, its refresh lifecycle,
 * and the saved-scan list. Owns NOTHING about scans or browsing; the
 * home view and the destination dialog subscribe here.
 *
 * Refresh semantics (EaseUS DrwHomePage parity):
 *  - boot: enumerate + saved scans in parallel
 *  - manual refresh (button / troubleshooting flyout)
 *  - focus re-warm: window focus triggers a quiet re-enumeration when
 *    the last one is older than 5 s (drives get plugged in meanwhile)
 */

import { create } from "zustand";
import { invoke } from "../lib/ipc";
import type { DiskInfo, SavedScanSummary } from "../bindings";

const REWARM_AFTER_MS = 5000;

interface DevicesStore {
  disks: DiskInfo[];
  disksLoading: boolean;
  disksError: string | null;
  /** Wall clock of the last successful enumeration. */
  lastLoadedAt: number;
  refresh: (opts?: { quiet?: boolean }) => Promise<void>;

  savedScans: SavedScanSummary[];
  savedLoading: boolean;
  refreshSaved: () => Promise<void>;
  deleteSaved: (scanId: string) => Promise<void>;

  /** Install the focus re-warm listener (idempotent; app-level). */
  ensureAutoRefresh: () => void;
  autoRefreshReady: boolean;
}

let rewarmTimer: number | null = null;

export const useDevicesStore = create<DevicesStore>((set, get) => ({
  disks: [],
  disksLoading: true,
  disksError: null,
  lastLoadedAt: 0,

  refresh: async (opts) => {
    if (!opts?.quiet) set({ disksLoading: true });
    try {
      const res = await invoke<{ disks: DiskInfo[] }>("list_disks", { req: {} });
      set({
        disks: res.disks ?? [],
        disksError: null,
        disksLoading: false,
        lastLoadedAt: Date.now(),
      });
    } catch (e) {
      set({
        disksError: String(e),
        disksLoading: false,
      });
    }
  },

  savedScans: [],
  savedLoading: true,

  refreshSaved: async () => {
    set({ savedLoading: true });
    try {
      const res = await invoke<{ scans: SavedScanSummary[] }>("list_saved_scans", {
        req: {},
      });
      set({ savedScans: res.scans ?? [], savedLoading: false });
    } catch {
      set({ savedLoading: false });
    }
  },

  deleteSaved: async (scanId) => {
    await invoke("delete_saved_scan", { req: { scan_id: scanId } }).catch(() => undefined);
    set((st) => ({ savedScans: st.savedScans.filter((s) => s.scan_id !== scanId) }));
  },

  autoRefreshReady: false,

  ensureAutoRefresh: () => {
    if (get().autoRefreshReady) return;
    set({ autoRefreshReady: true });

    const rewarm = () => {
      if (rewarmTimer != null) window.clearTimeout(rewarmTimer);
      rewarmTimer = window.setTimeout(() => {
        const st = get();
        if (Date.now() - st.lastLoadedAt >= REWARM_AFTER_MS && !st.disksLoading) {
          void st.refresh({ quiet: true });
        }
      }, 400);
    };
    window.addEventListener("focus", rewarm);

    // Initial load.
    void get().refresh();
    void get().refreshSaved();
  },
}));
