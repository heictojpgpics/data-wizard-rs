/**
 * Tour driver (CI-only): DATAWIZARD_TOUR=1 (via the Rust get_dev_hooks
 * command, or ?tour=1 in browser-dev) auto-cycles through every stage,
 * filter, dialog and theme with a title-stamped dwell so CI screenshot
 * passes can capture every state of the real app without interactive
 * automation.
 *
 * The harness sync signal: `Data Wizard · tNN-name` per step. Native
 * capture scripts (Win32 GetWindowText polling) shoot exactly one
 * named frame per step ("one for one page/mode/action"). Slugs stay
 * filesystem-safe.
 */
import { useEffect } from "react";
import { invoke } from "../lib/ipc";
import { useAppStore } from "../state/app";
import { useScanStore } from "../state/scan";
import { applyThemeAttributes, type ThemeId } from "../theme/themes";

interface TourHooks {
  tour: boolean;
  tour_mode: string | null;
  scan: string | null;
}

interface Step {
  name: string;
  apply: () => void;
  /** Dwell multiplier (x DWELL_MS). Theme steps keep 2x for the
   * crossfade; the scan steps wait for engine completion instead. */
  dwell?: number;
  /** Wait for the scan store to reach a terminal phase after apply(). */
  waitForScan?: boolean;
  /** Stamp the title BEFORE waiting (captures the live mid-scan state
   * instead of the terminal one). */
  stampBeforeWait?: boolean;
}

const DWELL_MS = 2600;

/** The harness sync signal: stamps `Data Wizard · tNN-name` into the
 * window title. document.title does NOT propagate to the native
 * window title on WebView2 — drive the real window via the Tauri API
 * too (the plain-browser mock keeps the document.title path; the
 * dynamic import resolves only inside the webview). */
function tourTitle(i: number, name: string) {
  const slug = name.replace(/[^a-z0-9]+/gi, "-").replace(/^-+|-+$/g, "").toLowerCase();
  const title = `Data Wizard · t${String(i).padStart(2, "0")}-${slug}`;
  document.title = title;
  import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
    .catch(() => {
      // Plain browser (mock) — document.title is the channel.
    });
}

/** The tour-done marker uses the same dual channel. */
function tourTitleEnd() {
  const title = "Data Wizard · tour-done";
  document.title = title;
  import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
    .catch(() => undefined);
}

export function TourDriver() {
  useEffect(() => {
    let timer: number | null = null;
    let disposed = false;

    void (async () => {
      const hooks = await invoke<TourHooks>("get_dev_hooks", { req: {} }).catch(() => ({
        tour: false,
        tour_mode: null,
        scan: null,
      }));
      if (!hooks?.tour || disposed) return;

      const app = useAppStore.getState();
      // Note: hooks.scan (DATAWIZARD_SCAN) is consumed by the
      // scan-start step below — the source-view steps run first so the
      // capture sequence reads: source -> themes -> scanning -> results.

      const waitScanTerminal = async (maxPolls = 240): Promise<void> => {
        for (let i = 0; i < maxPolls; i++) {
          const p = useScanStore.getState().progress?.phase;
          if (p === "Done" || p === "Failed" || p === "Cancelled") return;
          await new Promise((r) => window.setTimeout(r, 500));
        }
      };

      /** The browser-mock fallback source when DATAWIZARD_SCAN is unset:
       * the first non-system disk from the (mock) inventory. */
      const pickDefaultSource = (): string => {
        const disks = app.disks;
        const target = disks.find((d) => !d.is_system) ?? disks[0];
        return target?.id ?? "\\\\.\\PhysicalDrive1";
      };

      const startTourScan = async () => {
        const scan = useScanStore.getState();
        try {
          const scanId = await scan.startScan(hooks.scan ?? pickDefaultSource(), false, null);
          useAppStore.getState().beginScan(scanId, false, {
            uri: hooks.scan ?? "tour-source",
            label: (hooks.scan ?? "Tour source").split(/[\\/]/).pop() ?? "Tour source",
            detail: hooks.scan ? "CI staged source" : "Synthetic source",
            partition: null,
          });
        } catch {
          /* the no-scan tour continues */
        }
      };

      const steps: Step[] = [];

      // ── Source view states ──────────────────────────────────────────
      steps.push({ name: "source-home", apply: () => useAppStore.getState().clearScan() });
      steps.push({
        name: "source-partitions-open",
        apply: () => {
          // Expand the first disk card: the tour dispatches a DOM click
          // on the first disk head (the click handler is attached there).
          const head = document.querySelector<HTMLElement>(".dw-disk-head");
          head?.click();
        },
      });

      // ── Theme flips (all five; both attributes together) ───────────
      const themeSteps: ThemeId[] = ["dark", "ember", "tide", "blossom", "light"];
      for (const t of themeSteps) {
        steps.push({
          name: `theme-${t}`,
          dwell: 2,
          apply: () => applyThemeAttributes(t),
        });
      }

      // ── Scanning view (the real engine; title stamps mid-scan so the
      //    frame captures the live orb + timeline + feed) ─────────────
      steps.push({
        name: "scan-running",
        apply: () => {
          void startTourScan();
        },
        waitForScan: true,
        stampBeforeWait: true,
        dwell: 2,
      });

      // ── Results view states ────────────────────────────────────────
      steps.push({
        name: "results-table",
        apply: () => useAppStore.getState().setStage("results"),
      });
      steps.push({
        name: "results-category-photos",
        apply: () => useAppStore.getState().setCategoryFilter("photos"),
      });
      steps.push({
        name: "results-search",
        apply: () => {
          useAppStore.getState().setCategoryFilter("all");
          useAppStore.getState().setSearch("IMG");
        },
      });
      steps.push({
        name: "results-selection",
        apply: () => {
          useAppStore.getState().setSearch("");
          const rows = useAppStore.getState().rows;
          useAppStore.getState().setSelectedMany(rows.slice(0, 3).map((r) => r.id), true);
        },
      });
      steps.push({
        name: "results-inspector",
        apply: () => {
          const rows = useAppStore.getState().rows;
          if (rows.length > 0) useAppStore.getState().setInspectedId(rows[0].id);
        },
      });
      steps.push({
        name: "recovery-dialog",
        apply: () => {
          const st = useAppStore.getState();
          if (st.selected.size === 0) {
            st.setSelectedMany(st.rows.slice(0, 3).map((r) => r.id), true);
          }
          st.setRecoveryOpen(true);
        },
      });
      steps.push({
        name: "recovery-dialog-filled",
        apply: () => {
          // Fill the destination through the dialog's picker path: the
          // mock accepts the stand-in; the real app uses
          // DW_TOUR_DEST when provided (the dialog reads it).
          window.dispatchEvent(new CustomEvent("dw-tour-destination", { detail: { path: "D:\\Recovered" } }));
        },
        dwell: 2,
      });

      // ── Run ─────────────────────────────────────────────────────────
      let i = 0;
      const runStep = async () => {
        if (disposed) return;
        if (i >= steps.length) {
          tourTitleEnd();
          return;
        }
        const step = steps[i];
        step.apply();
        if (step.stampBeforeWait) {
          // Mid-scan capture: stamp first so the harness shoots while
          // the engine is still running.
          tourTitle(i, step.name);
        }
        if (step.waitForScan) {
          await waitScanTerminal();
        }
        if (!step.stampBeforeWait) {
          tourTitle(i, step.name);
        }
        i += 1;
        timer = window.setTimeout(() => void runStep(), DWELL_MS * (step.dwell ?? 1));
      };
      await runStep();
    })();

    return () => {
      disposed = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, []);

  return null;
}
