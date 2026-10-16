/**
 * Data Wizard app shell (revamp): 56px top bar (brand, wizard step
 * tracker, window drag region, Windows caption buttons) → body (the
 * stage view owns its own side panes) → 30px status strip. Hosts the
 * three wizard stages, the recovery flow dialogs, toasts, tooltips,
 * confirms, and the DATAWIZARD_TOUR driver (CI screenshot tours).
 */
import { useEffect, useState } from "react";
import { TopBar } from "./shell/TopBar";
import { StatusBar } from "./shell/StatusBar";
import { HomeView } from "./views/HomeView";
import { ScanningView } from "./views/ScanningView";
import { ResultsView } from "./views/ResultsView";
import { FileInspector } from "./inspector/FileInspector";
import { DestinationDialog } from "./components/DestinationDialog";
import { RecoveryFlow } from "./components/RecoveryFlow";
import { Toasts } from "./components/Toasts";
import { Tooltip } from "./components/ui/Tooltip";
import { ConfirmDialog } from "./components/ui/ConfirmDialog";
import { TourDriver } from "./shell/TourDriver";
import { useAppStore } from "./state/app";
import { useScanStore } from "./state/scan";
import { useDevicesStore } from "./state/devices";
import { useUiStore } from "./state/ui";

import "./theme/tokens.css";
import "./styles/base.css";
import "./styles/kit.css";
import "./styles/shell.css";
import "./styles/home.css";
import "./styles/scan.css";
import "./styles/results.css";
import "./styles/inspector.css";
import "./styles/overlays.css";

/** Stage swap: the entering view fades 0→1 over the solid page
 * background; the old view unmounts INSTANTLY (conditional render —
 * no exit tween, no lingering layer). */
function StageSwap({ children }: { children: React.ReactNode }) {
  return <div className="dw-tab-swap">{children}</div>;
}

function AppShell() {
  const stage = useAppStore((s) => s.stage);
  const scanId = useAppStore((s) => s.scanId);

  const [inspectedId, setInspectedId] = useState<string | null>(null);
  const confirm = useUiStore((s) => s.confirm);
  const closeConfirm = useUiStore((s) => s.closeConfirm);

  // Boot: persistent listeners + device/saved-scan warm-up.
  useEffect(() => {
    useScanStore.getState().ensureListeners();
    useDevicesStore.getState().ensureAutoRefresh();
  }, []);

  // The inspector id can be invalidated by a store reset — clamp.
  useEffect(() => {
    if (inspectedId != null && stage === "results") {
      // No-op when valid; the inspector itself renders null if missing.
    }
  }, [inspectedId, stage]);

  return (
    <div className="dw-app">
      <TopBar />
      <main className="dw-main dw-main-full">
        <StageSwap key={stage}>
          {stage === "source" && <HomeView />}
          {stage === "scanning" && <ScanningView />}
          {stage === "results" && scanId && (
            <ResultsView onInspect={(id) => setInspectedId(id)} />
          )}
        </StageSwap>
      </main>
      {stage === "results" && inspectedId && (
        <FileInspector id={inspectedId} onClose={() => setInspectedId(null)} />
      )}
      <StatusBar />
      <DestinationDialog />
      <RecoveryFlow />
      {confirm && <ConfirmDialog spec={confirm} onClose={closeConfirm} />}
      <Toasts />
      <Tooltip />
      <TourDriver />
    </div>
  );
}

export default AppShell;
