/**
 * Tour driver (CI-only): DATAWIZARD_TOUR=1 (via the Rust get_dev_hooks
 * command, or ?tour=1 in browser-dev) auto-cycles through every stage,
 * view mode, filter, dialog and theme with a title-stamped dwell so CI
 * screenshot passes can capture every state of the real app without
 * interactive automation.
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
import { useResultsStore } from "../state/results";
import { useRecoveryStore } from "../state/recovery";
import { useDevicesStore } from "../state/devices";
import { useUiStore } from "../state/ui";
import { applyThemeAttributes, type ThemeId } from "../theme/themes";

interface TourHooks {
  tour: boolean;
  tour_mode: string | null;
  scan: string | null;
}

interface Step {
  name: string;
  apply: () => void;
  /** Dwell multiplier (x DWELL_MS). */
  dwell?: number;
  /** Wait for the scan to reach a terminal phase after apply(). */
  waitForScan?: boolean;
  /** Stamp the title BEFORE waiting (mid-scan capture). */
  stampBeforeWait?: boolean;
}

const DWELL_MS = 2600;

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

      const waitScanTerminal = async (maxPolls = 240): Promise<void> => {
        for (let i = 0; i < maxPolls; i++) {
          const p = useScanStore.getState().phase;
          if (p === "done" || p === "failed" || p === "cancelled") return;
          await new Promise((r) => window.setTimeout(r, 500));
        }
      };

      const pickDefaultSource = (): string => {
        const disks = useDevicesStore.getState().disks;
        const target = disks.find((d) => !d.is_system) ?? disks[0];
        return target?.id ?? "\\\\.\\PhysicalDrive1";
      };

      const startTourScan = async () => {
        const scan = useScanStore.getState();
        try {
          await scan.start({
            source: {
              uri: hooks.scan ?? pickDefaultSource(),
              label: (hooks.scan ?? "Tour source").split(/[\\/]/).pop() ?? "Tour source",
              detail: hooks.scan ? "CI staged source" : "Synthetic source",
              partition: null,
            },
            deep: true,
          });
        } catch (e) {
          useUiStore.getState().pushToast({
            kind: "err",
            title: "Tour scan failed to start",
            body: String(e),
          });
        }
      };

      const steps: Step[] = [];

      // ── Home states ────────────────────────────────────────────────
      steps.push({ name: "source-home", apply: () => useAppStore.getState().clearScan() });
      steps.push({
        name: "source-partitions-open",
        apply: () => {
          const head = document.querySelector<HTMLElement>(".dw-disk-id");
          head?.click();
        },
        dwell: 1,
      });
      steps.push({
        name: "source-trouble-flyout",
        apply: () => {
          const btns = [...document.querySelectorAll<HTMLElement>("button")];
          btns.find((b) => b.textContent?.includes("Can't find a location"))?.click();
        },
      });

      // ── Theme flips ────────────────────────────────────────────────
      const themeSteps: ThemeId[] = ["dark", "ember", "tide", "blossom", "light"];
      for (const t of themeSteps) {
        steps.push({
          name: `theme-${t}`,
          dwell: 2,
          apply: () => applyThemeAttributes(t),
        });
      }

      // ── Scanning (mid-scan capture, then wait for terminal) ────────
      steps.push({
        name: "scan-running",
        apply: () => {
          void startTourScan();
        },
        waitForScan: true,
        stampBeforeWait: true,
        dwell: 2,
      });

      // ── Results states ─────────────────────────────────────────────
      const R = () => useResultsStore.getState();

      steps.push({ name: "results-table", apply: () => useAppStore.getState().setStage("results") });
      steps.push({
        name: "results-type-photos",
        apply: () => R().navigate({ view: "type", path: "/", category: "photos" }),
      });
      steps.push({
        name: "results-special-deleted",
        apply: () => {
          R().navigate({ view: "type", path: "/", category: null });
          R().setFilter({ special: "deleted" });
        },
      });
      steps.push({
        name: "results-path-view",
        apply: () => {
          R().setFilter({ special: null });
          R().navigate({ view: "path", path: "/", category: null });
        },
      });
      steps.push({
        name: "results-grid",
        apply: () => R().navigate({ view: "grid", path: "/", category: "photos" }),
      });
      steps.push({
        name: "results-search",
        apply: () => {
          R().navigate({ view: "type", path: "/", category: null });
          R().setFilter({ search: "IMG" });
        },
      });
      steps.push({
        name: "results-filter-drawer",
        apply: () => {
          R().setFilter({ search: "", hideJunk: true, deletedOnly: true });
          const btns = [...document.querySelectorAll<HTMLElement>("button")];
          btns.find((b) => b.textContent?.trim() === "Filter")?.click();
        },
        dwell: 1,
      });
      steps.push({
        name: "results-selection",
        apply: () => {
          R().clearFilter();
          const byId = R().byId;
          const ids = [...byId.keys()].slice(0, 3);
          R().selectMany(ids, true);
        },
      });

      // ── Recovery flow (destination shown; no actual run in CI) ─────
      steps.push({
        name: "recovery-destination",
        apply: () => useRecoveryStore.getState().open("selected"),
      });
      steps.push({
        name: "recovery-destination-filled",
        apply: () => {
          const rows = document.querySelectorAll<HTMLElement>(".dw-dest-row:not(.dw-blocked)");
          rows[0]?.click();
        },
        dwell: 2,
      });

      // ── Run ────────────────────────────────────────────────────────
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
