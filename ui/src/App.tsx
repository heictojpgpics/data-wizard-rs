/**
 * Data Wizard app shell: 56px top bar (brand, wizard step tracker,
 * window drag region, Windows caption buttons) → body (contextual
 * sidebar | main | inspector on results) → 30px status strip. Hosts
 * the three wizard stages, the recovery dialog, toasts, and the
 * DATAWIZARD_TOUR driver (CI screenshot tours).
 */
import { useCallback, useEffect } from "react";
import type { SavedScanSummary } from "./bindings";
import { TopBar } from "./shell/TopBar";
import { StatusBar, warmDisks } from "./shell/StatusBar";
import { Sidebar } from "./sidebar/Sidebar";
import { SelectSource } from "./views/SelectSource";
import { ScanningView } from "./views/ScanningView";
import { ResultsView } from "./views/ResultsView";
import { FileInspector } from "./inspector/FileInspector";
import { RecoveryDialog } from "./components/RecoveryDialog";
import { Toasts } from "./components/Toasts";
import { TourDriver } from "./shell/TourDriver";
import { useAppStore, type ChosenSource } from "./state/app";
import { useScanStore } from "./state/scan";
import { invoke } from "./lib/ipc";

import "./theme/tokens.css";
import "./styles/base.css";
import "./styles/shell.css";
import "./styles/sidebar.css";
import "./styles/source.css";
import "./styles/scan.css";
import "./styles/results.css";
import "./styles/inspector.css";
import "./styles/overlays.css";

/** Tab/stage swap wrapper: the entering view fades 0->1 over the solid
 * page background; the old view unmounts INSTANTLY (conditional
 * render — no exit tween, no lingering layer). The wrapper carries
 * the solid background so the fade never flashes. */
function StageSwap({ children }: { children: React.ReactNode }) {
  return <div className="dw-tab-swap">{children}</div>;
}

function AppShell() {
  const stage = useAppStore((s) => s.stage);
  const inspectorVisible = useAppStore((s) => s.inspectorVisible);
  const scannedId = useAppStore((s) => s.scanId);
  const recoveryOpen = useAppStore((s) => s.recoveryOpen);
  const toast = useAppStore((s) => s.toast);

  const startScan = useCallback(async (source: ChosenSource, deep: boolean) => {
    const scan = useScanStore.getState();
    try {
      const scanId = await scan.startScan(source.uri, deep, source.partition);
      useAppStore.getState().beginScan(scanId, deep, source);
    } catch (e) {
      useAppStore.getState().setToast({ text: `Could not start the scan: ${String(e)}`, kind: "err" });
    }
  }, []);

  const resumeScan = useCallback(async (saved: SavedScanSummary) => {
    const scan = useScanStore.getState();
    try {
      const scanId = await scan.resumeScan(saved.scan_id, saved.mode === "deep");
      const label = saved.source_uri.split(/[\\/]/).pop() ?? saved.source_uri;
      useAppStore.getState().beginScan(scanId, saved.mode === "deep", {
        uri: saved.source_uri,
        label,
        detail: "Resumed saved scan",
        partition: null,
      });
    } catch (e) {
      useAppStore.getState().setToast({ text: `Could not resume: ${String(e)}`, kind: "err" });
    }
  }, []);

  // Boot: install the persistent scan listeners + warm the disk list.
  useEffect(() => {
    useScanStore.getState().ensureListeners();
    warmDisks();
    // Best-effort saved scans warm (the sidebar also loads its own).
    invoke<{ scans: SavedScanSummary[] }>("list_saved_scans", { req: {} })
      .then((res) => {
        useAppStore.getState().setSavedScans(res.scans ?? []);
        useAppStore.getState().setSavedLoading(false);
      })
      .catch(() => useAppStore.getState().setSavedLoading(false));
  }, []);

  return (
    <div className="dw-app">
      <TopBar />
      <div className="dw-body">
        <Sidebar />
        <main className="dw-main">
          <StageSwap key={stage}>
            {stage === "source" && <SelectSource onScan={startScan} onResume={resumeScan} />}
            {stage === "scanning" && (
              <ScanningView onViewResults={() => useAppStore.getState().setStage("results")} />
            )}
            {stage === "results" && scannedId && (
              <ResultsView onBack={() => useAppStore.getState().clearScan()} />
            )}
          </StageSwap>
        </main>
        {stage === "results" && inspectorVisible && <FileInspector />}
      </div>
      <StatusBar />
      {recoveryOpen && <RecoveryDialog />}
      {toast && <Toasts key={toast.id} />}
      <TourDriver />
    </div>
  );
}

export default AppShell;
