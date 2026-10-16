/**
 * Status strip — the 30px bottom bar: app state at a glance (stage,
 * scan phase / results count / selection), the safety reminder, and
 * the version. Grounds the app like a professional tool.
 */

import { useAppStore } from "../state/app";
import { useScanStore } from "../state/scan";
import { useResultsStore } from "../state/results";
import { formatBytes, formatCount } from "../lib/format";

const VERSION = "0.1.0";

export function StatusBar() {
  const stage = useAppStore((s) => s.stage);
  const scanDeep = useAppStore((s) => s.scanDeep);
  const phase = useScanStore((s) => s.phase);
  const paused = useScanStore((s) => s.paused);
  const found = useScanStore((s) => s.found);

  const total = useResultsStore((s) => s.total);
  const selected = useResultsStore((s) => s.selected);
  const byId = useResultsStore((s) => s.byId);

  let left: React.ReactNode;
  let dot: "live" | "ok" | "err" | null = null;

  if (stage === "source") {
    left = <span className="dw-status-item">Ready — pick a source to begin</span>;
  } else if (stage === "scanning") {
    const label =
      phase === "done"
        ? "Scan complete"
        : phase === "failed"
          ? "Scan failed"
          : phase === "cancelled"
            ? "Scan cancelled"
            : paused
              ? "Scan paused"
              : `Scanning — ${found.toLocaleString()} found`;
    dot = phase === "failed" ? "err" : phase === "done" || phase === "cancelled" ? "ok" : "live";
    left = (
      <span className="dw-status-item tnum">
        {label}
        {scanDeep && phase !== "done" ? " · deep mode" : ""}
      </span>
    );
  } else {
    dot = "ok";
    let selBytes = 0;
    for (const id of selected) selBytes += byId.get(id)?.file.size_bytes ?? 0;
    left = (
      <span className="dw-status-item tnum">
        {formatCount(total)} results · {formatCount(selected.size)} selected
        {selected.size > 0 ? ` · ${formatBytes(selBytes)}` : ""}
      </span>
    );
  }

  return (
    <footer className="dw-statusbar">
      <span className={`dw-status-dot${dot ? ` dw-${dot}` : ""}`} aria-hidden />
      {left}
      <span className="dw-status-spacer" />
      <span className="dw-status-item dw-status-muted">
        Read-only engine · recovers to the destination you choose
      </span>
      <span className="dw-status-item dw-status-muted">v{VERSION}</span>
    </footer>
  );
}
