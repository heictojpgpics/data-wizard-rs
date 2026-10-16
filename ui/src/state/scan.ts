/**
 * Scan store — the lifecycle brain (rewritten for the revamp).
 *
 * Owns the phase machine with EaseUS semantics:
 *
 *   Probing → QuickScan ──(deep?)──▶ DeepScan/Carving → Finalizing → Done
 *                 │                                │
 *                 └── quick-complete interstitial ◀─┘  (the banner view)
 *
 * Plus:
 *   - the elapsed clock (1 Hz tick, pause-aware)
 *   - rate/ETA measured from progress deltas
 *   - the LIVE PUMP: polls list_results during the scan and streams
 *     rows into the results store (tree + counters update in realtime)
 *   - pause/resume (Rust set_scan_paused; the source gates its reads)
 *   - the completion sequence: organizing (bulk-load) → results stage
 */

import { create } from "zustand";
import { invoke, listen } from "../lib/ipc";
import type { RecoveredFile, ScanPhase, ScanProgress } from "../bindings";
import { useAppStore, type ChosenSource } from "./app";
import { useResultsStore } from "./results";

/** Derived display phase (the view's state machine). */
export type DisplayPhase =
  | "idle"
  | "probing"
  | "quick"
  | "quickComplete" // interstitial: quick done, deep pending
  | "deep"
  | "finalizing"
  | "done"
  | "cancelled"
  | "failed";

const POLL_MS = 350;
const PAGE = 400;

interface ScanStore {
  progress: ScanProgress | null;
  phase: DisplayPhase;
  paused: boolean;
  elapsedMs: number;
  ratePerSec: number | null;
  etaSec: number | null;
  /** Total entries the engine has found so far (from polling). */
  found: number;

  listenersReady: boolean;
  ensureListeners: () => void;

  start: (opts: { source: ChosenSource; deep: boolean }) => Promise<void>;
  resume: (savedScanId: string, deep: boolean, source: ChosenSource) => Promise<void>;
  pause: () => Promise<void>;
  unpause: () => Promise<void>;
  stop: () => Promise<void>;
  /** Dismiss the quick-complete banner (deep scan keeps running). */
  dismissQuickComplete: () => void;

  /** Internal: driven by scan:progress events. */
  _onProgress: (p: ScanProgress) => void;
  _tick: () => void;
  _pump: () => Promise<void>;
}

/** Module-scope pump state (not reactive). */
interface PumpState {
  loaded: number;
  timer: number | null;
  lastCount: number;
  lastAt: number;
  startedAt: number;
  pausedAccum: number;
  pausedAt: number;
  phase: ScanPhase | "idle";
}
let pump: PumpState = {
  loaded: 0,
  timer: null,
  lastCount: 0,
  lastAt: 0,
  startedAt: 0,
  pausedAccum: 0,
  pausedAt: 0,
  phase: "idle",
};

function stopPump() {
  if (pump.timer != null) {
    window.clearTimeout(pump.timer);
    pump.timer = null;
  }
}

function schedulePump() {
  if (pump.timer != null) return;
  pump.timer = window.setTimeout(async () => {
    pump.timer = null;
    await useScanStore.getState()._pump();
    const st = useScanStore.getState();
    if (
      st.phase === "quick" ||
      st.phase === "deep" ||
      st.phase === "probing" ||
      st.phase === "finalizing" ||
      st.phase === "quickComplete"
    ) {
      schedulePump();
    }
  }, POLL_MS);
}

function resetPump() {
  stopPump();
  pump = {
    loaded: 0,
    timer: null,
    lastCount: 0,
    lastAt: 0,
    startedAt: Date.now(),
    pausedAccum: 0,
    pausedAt: 0,
    phase: "idle",
  };
}

function displayPhaseOf(p: ScanPhase | "idle", prev: DisplayPhase): DisplayPhase {
  switch (p) {
    case "Probing":
      return "probing";
    case "QuickScan":
      return "quick";
    case "DeepScan":
    case "Carving":
      // The interstitial only shows on the FIRST quick→deep transition.
      return prev === "quick" || prev === "quickComplete" ? "quickComplete" : "deep";
    case "Finalizing":
      return "finalizing";
    case "Done":
      return "done";
    case "Cancelled":
      return "cancelled";
    case "Failed":
      return "failed";
    default:
      return prev;
  }
}

export const useScanStore = create<ScanStore>((set, get) => ({
  progress: null,
  phase: "idle",
  paused: false,
  elapsedMs: 0,
  ratePerSec: null,
  etaSec: null,
  found: 0,

  listenersReady: false,

  ensureListeners: () => {
    if (get().listenersReady) return;
    set({ listenersReady: true });

    void listen<ScanProgress>("scan:progress", (p) => get()._onProgress(p));

    // 1 Hz clock while a scan is running.
    window.setInterval(() => get()._tick(), 1000);
  },

  _onProgress: (p) => {
    const app = useAppStore.getState();
    if (!app.scanId || p.scan_id !== app.scanId) return;

    const prev = get().phase;
    const next = displayPhaseOf(p.phase, prev);
    pump.phase = p.phase;

    // Rate/ETA from the found counter over ≥1.2s spans.
    const now = Date.now();
    if (p.current > 0) {
      if (pump.lastCount === 0) {
        pump.lastCount = p.current;
        pump.lastAt = now;
      } else if (p.current > pump.lastCount) {
        const dt = (now - pump.lastAt) / 1000;
        if (dt >= 1.2) {
          const rate = (p.current - pump.lastCount) / dt;
          pump.lastCount = p.current;
          pump.lastAt = now;
          const total = p.total > 0 ? p.total : null;
          set({
            ratePerSec: rate,
            etaSec: total ? Math.max(0, Math.round((total - p.current) / Math.max(rate, 0.01))) : null,
          });
        }
      }
    }

    if (next !== prev) {
      set({ phase: next });

      if (next === "done" || next === "cancelled" || next === "failed") {
        stopPump();
        set({ paused: false });
        // Completion sequence: final pump + organize, then advance.
        void finishScan(next);
      }

      if (next === "quickComplete" && prev === "quick") {
        // Quick finished: results are already browsable; the pump keeps
        // streaming while deep runs.
        useAppStore.getState().setStage("results");
        useResultsStore.getState().setOrganizing(false);
      }
    }

    set({ progress: p, found: Math.max(get().found, p.current) });

    /* Fallback ETA: before a measured rate exists (the first ~1.2s),
     * extrapolate from the progress fraction once it clears 5% — a
     * stalled "estimating…" deep into the scan reads as broken (the
     * VLM pass flagged it at 17%). */
    if (get().etaSec == null && p.total > 0 && p.current / p.total > 0.05) {
      const elapsedS = (Date.now() - pump.startedAt - pump.pausedAccum) / 1000;
      if (elapsedS > 0.5) {
        const fraction = p.current / p.total;
        const est = Math.max(1, Math.round((elapsedS / fraction) * (1 - fraction)));
        set({ etaSec: est });
      }
    }
  },

  _tick: () => {
    const st = get();
    if (st.phase === "done" || st.phase === "cancelled" || st.phase === "failed" || st.phase === "idle") {
      return;
    }
    const pausedSpan = st.paused && pump.pausedAt > 0 ? Date.now() - pump.pausedAt : 0;
    const elapsed = Date.now() - pump.startedAt - pump.pausedAccum - pausedSpan;
    set({ elapsedMs: Math.max(0, elapsed) });
  },

  _pump: async () => {
    const app = useAppStore.getState();
    const scanId = app.scanId;
    if (!scanId) return;
    try {
      const res = await invoke<{
        items: RecoveredFile[];
        total: number;
        spilled: number;
      }>("list_results", {
        req: { scan_id: scanId, offset: pump.loaded, limit: PAGE, extension_filter: null },
      });
      const items = res.items ?? [];
      const results = useResultsStore.getState();
      if (items.length > 0) {
        results.ingest(items);
        pump.loaded += items.length;
      }
      results.setTotal(res.total ?? 0, res.spilled ?? 0);
      set({ found: res.total ?? pump.loaded });
    } catch {
      /* transient (scan record lock contention) — next tick retries */
    }
  },

  start: async ({ source, deep }) => {
    resetPump();
    useResultsStore.getState().reset(0);
    set({
      progress: null,
      phase: "probing",
      paused: false,
      elapsedMs: 0,
      ratePerSec: null,
      etaSec: null,
      found: 0,
    });
    const res = await invoke<{ scan_id: string }>("start_scan", {
      req: {
        source_uri: source.uri,
        deep,
        max_results: 1_000_000,
        file_filter: [],
        partition: source.partition ?? undefined,
      },
    });
    // Identity + stage live in the app store: beginScan stamps the
    // scanId EVERY downstream consumer (events, results, recovery)
    // matches against.
    useAppStore.getState().beginScan(res.scan_id, deep, source);
    // Quick-target folder filters (Recycle Bin / specific folder) apply
    // as a path search the moment results exist.
    if (source.pathPrefix) {
      useResultsStore.getState().setFilter({ search: source.pathPrefix });
    }
    schedulePump();
  },

  resume: async (savedScanId, deep, source) => {
    resetPump();
    set({
      progress: null,
      phase: "probing",
      paused: false,
      elapsedMs: 0,
      ratePerSec: null,
      etaSec: null,
      found: 0,
    });
    const res = await invoke<{ scan_id: string }>("resume_scan", {
      req: { saved_scan_id: savedScanId, deep },
    });
    useAppStore.getState().beginScan(res.scan_id, deep, source);
    schedulePump();
  },

  pause: async () => {
    const scanId = useAppStore.getState().scanId;
    if (!scanId || get().paused) return;
    await invoke("set_scan_paused", { req: { scan_id: scanId, paused: true } }).catch(() => undefined);
    pump.pausedAt = Date.now();
    set({ paused: true });
  },

  unpause: async () => {
    const scanId = useAppStore.getState().scanId;
    if (!scanId || !get().paused) return;
    await invoke("set_scan_paused", { req: { scan_id: scanId, paused: false } }).catch(() => undefined);
    if (pump.pausedAt > 0) {
      pump.pausedAccum += Date.now() - pump.pausedAt;
      pump.pausedAt = 0;
    }
    set({ paused: false });
  },

  stop: async () => {
    const scanId = useAppStore.getState().scanId;
    if (!scanId) return;
    stopPump();
    await invoke("cancel_scan", { req: { scan_id: scanId } }).catch(() => undefined);
  },

  dismissQuickComplete: () =>
    set((st) => (st.phase === "quickComplete" ? { phase: "deep" } : {})),
}));

/* ── Completion sequence ────────────────────────────────────────────────── */

async function finishScan(outcome: "done" | "cancelled" | "failed"): Promise<void> {
  const app = useAppStore.getState();
  const scanId = app.scanId;
  if (!scanId) return;

  const results = useResultsStore.getState();

  if (outcome === "done") {
    // Organizing: bulk-load everything still unloaded, in chunks that
    // yield to the UI (EaseUS's "Organizing Found Files" moment).
    results.setOrganizing(true, 0);
    const CHUNK = 1000;
    for (;;) {
      let page: { items: RecoveredFile[]; total: number; spilled: number };
      try {
        page = await invoke("list_results", {
          req: { scan_id: scanId, offset: pump.loaded, limit: CHUNK, extension_filter: null },
        });
      } catch {
        break;
      }
      const items = page.items ?? [];
      if (items.length === 0) break;
      results.ingest(items);
      pump.loaded += items.length;
      results.setTotal(page.total ?? pump.loaded, page.spilled ?? 0);
      results.setOrganizing(true, page.total > 0 ? pump.loaded / page.total : null);
      if (items.length < CHUNK) break;
    }
    results.setOrganizing(false, null);
    results.setLoadedAll(true);
    app.setStage("results");
  } else if (outcome === "cancelled") {
    // Keep whatever was found; browsing a partial scan is legitimate.
    await useScanStore.getState()._pump();
    results.setLoadedAll(true);
    app.setStage("results");
  } else {
    app.setStage("source");
    useResultsStore.getState().reset(0);
  }
}

/* ── Formatting helpers for the views ──────────────────────────────────── */

export function formatClock(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(sec).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function formatEta(sec: number | null): string {
  if (sec == null || !Number.isFinite(sec)) return "estimating…";
  if (sec < 5) return "almost done";
  if (sec < 60) return `~${Math.ceil(sec / 5) * 5}s left`;
  if (sec < 3600) return `~${Math.round(sec / 60)} min left`;
  return `~${Math.floor(sec / 3600)}h ${Math.round((sec % 3600) / 60)}m left`;
}
