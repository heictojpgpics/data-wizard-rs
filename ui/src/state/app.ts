/**
 * App store — the wizard stage machine, source inventory, saved scans,
 * results browsing state (filters, search, sort, pagination), selection
 * and the inspector. One zustand store; the scan lifecycle lives in
 * state/scan.ts and cross-references this one.
 */
import { create } from "zustand";
import type {
  DiskInfo,
  RecoveredFile,
  SavedScanSummary,
} from "../bindings";
import type { CategoryId } from "../lib/format";

export type Stage = "source" | "scanning" | "results";

export type SortKey = "name" | "size" | "modified" | "status";

/** A source chosen for scanning (disk, partition, or image file). */
export interface ChosenSource {
  /** The URI passed to start_scan (disk id / \\.\ path / file path). */
  uri: string;
  /** Display name for headers. */
  label: string;
  /** Sub-label (bus / fs kind / size). */
  detail: string;
  /** Optional partition index within the source. */
  partition: number | null;
}

interface AppStore {
  /* ── Wizard stage ──────────────────────────────────────────────── */
  stage: Stage;
  setStage: (s: Stage) => void;

  /* ── Sources ───────────────────────────────────────────────────── */
  disks: DiskInfo[];
  disksLoading: boolean;
  disksError: string | null;
  setDisks: (d: DiskInfo[]) => void;
  setDisksLoading: (v: boolean) => void;
  setDisksError: (e: string | null) => void;

  savedScans: SavedScanSummary[];
  savedLoading: boolean;
  setSavedScans: (s: SavedScanSummary[]) => void;
  setSavedLoading: (v: boolean) => void;

  /* ── The current scan ─────────────────────────────────────────── */
  scanId: string | null;
  scanDeep: boolean;
  source: ChosenSource | null;
  /** Wall-clock ms when the current scan began (elapsed displays). */
  scanStartedAt: number;
  beginScan: (scanId: string, deep: boolean, source: ChosenSource) => void;
  clearScan: () => void;

  /* ── Results browsing ─────────────────────────────────────────── */
  rows: RecoveredFile[];
  total: number;
  spilled: number;
  loadingPage: boolean;
  exhausted: boolean;
  /** Rows streamed live during a scan (the "found feed"). */
  liveFeed: RecoveredFile[];

  categoryFilter: CategoryId | "all";
  statusFilter: string | null;
  search: string;
  sort: SortKey;
  sortAsc: boolean;

  setCategoryFilter: (c: CategoryId | "all") => void;
  setStatusFilter: (s: string | null) => void;
  setSearch: (q: string) => void;
  setSort: (k: SortKey, asc?: boolean) => void;

  /** Append a page of rows (dedup by id). */
  appendRows: (items: RecoveredFile[], total: number, spilled: number, reset: boolean) => void;
  setLoadingPage: (v: boolean) => void;
  setExhausted: (v: boolean) => void;
  pushLiveFeed: (f: RecoveredFile) => void;
  clearLiveFeed: () => void;
  resetBrowsing: () => void;

  /* ── Selection ────────────────────────────────────────────────── */
  selected: Set<string>;
  toggleSelected: (id: string) => void;
  setSelectedMany: (ids: string[], on: boolean) => void;
  clearSelection: () => void;

  /* ── Inspector ────────────────────────────────────────────────── */
  inspectedId: string | null;
  setInspectedId: (id: string | null) => void;
  inspectorVisible: boolean;
  setInspectorVisible: (v: boolean) => void;

  /* ── Overlays ─────────────────────────────────────────────────── */
  recoveryOpen: boolean;
  setRecoveryOpen: (v: boolean) => void;
  toast: { id: number; text: string; kind: "ok" | "err" | "info" } | null;
  setToast: (t: { text: string; kind: "ok" | "err" | "info" } | null) => void;
}

const PAGE = 200;

export const RESULTS_PAGE = PAGE;

export const useAppStore = create<AppStore>((set) => ({
  stage: "source",
  setStage: (s) => set({ stage: s }),

  disks: [],
  disksLoading: true,
  disksError: null,
  setDisks: (d) => set({ disks: d }),
  setDisksLoading: (v) => set({ disksLoading: v }),
  setDisksError: (e) => set({ disksError: e }),

  savedScans: [],
  savedLoading: true,
  setSavedScans: (s) => set({ savedScans: s }),
  setSavedLoading: (v) => set({ savedLoading: v }),

  scanId: null,
  scanDeep: false,
  source: null,
  scanStartedAt: 0,
  beginScan: (scanId, deep, source) =>
    set({
      scanId,
      scanDeep: deep,
      source,
      scanStartedAt: Date.now(),
      stage: "scanning",
      rows: [],
      total: 0,
      spilled: 0,
      exhausted: false,
      liveFeed: [],
      selected: new Set<string>(),
      inspectedId: null,
      recoveryOpen: false,
    }),
  clearScan: () =>
    set({
      scanId: null,
      source: null,
      stage: "source",
      rows: [],
      total: 0,
      spilled: 0,
      exhausted: false,
      liveFeed: [],
      selected: new Set<string>(),
      inspectedId: null,
      recoveryOpen: false,
      categoryFilter: "all",
      statusFilter: null,
      search: "",
    }),

  rows: [],
  total: 0,
  spilled: 0,
  loadingPage: false,
  exhausted: false,
  liveFeed: [],

  categoryFilter: "all",
  statusFilter: null,
  search: "",
  sort: "size",
  sortAsc: false,

  setCategoryFilter: (c) =>
    set({ categoryFilter: c, rows: [], total: 0, exhausted: false, selected: new Set<string>() }),
  setStatusFilter: (s) =>
    set({ statusFilter: s, rows: [], total: 0, exhausted: false, selected: new Set<string>() }),
  setSearch: (q) => set({ search: q }),
  setSort: (k, asc) =>
    set((st) => ({ sort: k, sortAsc: asc ?? (k === st.sort ? !st.sortAsc : k === "name") })),

  appendRows: (items, total, spilled, reset) =>
    set((st) => {
      const seen = reset ? new Set<string>() : new Set(st.rows.map((r) => r.id));
      const fresh = items.filter((r) => !seen.has(r.id));
      return {
        rows: reset ? fresh : [...st.rows, ...fresh],
        total,
        spilled,
        exhausted: reset ? items.length === 0 : st.rows.length + fresh.length >= total,
      };
    }),
  setLoadingPage: (v) => set({ loadingPage: v }),
  setExhausted: (v) => set({ exhausted: v }),
  pushLiveFeed: (f) =>
    set((st) => ({
      liveFeed: [f, ...st.liveFeed].slice(0, 24),
      rows: st.rows.some((r) => r.id === f.id) ? st.rows : [f, ...st.rows].slice(0, 600),
      total: st.rows.some((r) => r.id === f.id) ? st.total : st.total + 1,
    })),
  clearLiveFeed: () => set({ liveFeed: [] }),
  resetBrowsing: () =>
    set({
      rows: [],
      total: 0,
      spilled: 0,
      exhausted: false,
      selected: new Set<string>(),
      categoryFilter: "all",
      statusFilter: null,
      search: "",
      sort: "size",
      sortAsc: false,
    }),

  selected: new Set<string>(),
  toggleSelected: (id) =>
    set((st) => {
      const next = new Set(st.selected);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { selected: next };
    }),
  setSelectedMany: (ids, on) =>
    set((st) => {
      const next = new Set(st.selected);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return { selected: next };
    }),
  clearSelection: () => set({ selected: new Set<string>() }),

  inspectedId: null,
  setInspectedId: (id) => set({ inspectedId: id, inspectorVisible: id != null }),
  inspectorVisible: false,
  setInspectorVisible: (v) => set({ inspectorVisible: v }),

  recoveryOpen: false,
  setRecoveryOpen: (v) => set({ recoveryOpen: v }),
  toast: null,
  setToast: (t) => set({ toast: t ? { ...t, id: Date.now() } : null }),
}));
