/**
 * Status strip — the 30px bottom bar: app state at a glance (stage,
 * scan phase / results count / selection), the safety reminder, and
 * the version. Grounds the app like a professional tool.
 */
import { invoke } from "../lib/ipc";
import { useAppStore } from "../state/app";
import { useScanStore } from "../state/scan";
import { formatBytes, formatCount } from "../lib/format";

const VERSION = "0.1.0";

export function StatusBar() {
  const stage = useAppStore((s) => s.stage);
  const rows = useAppStore((s) => s.rows);
  const total = useAppStore((s) => s.total);
  const selected = useAppStore((s) => s.selected);
  const scanDeep = useAppStore((s) => s.scanDeep);
  const phase = useScanStore((s) => s.progress?.phase ?? null);

  let left: React.ReactNode;
  let dot: "live" | "ok" | "err" | null = null;

  if (stage === "source") {
    left = <span className="dw-status-item">Ready — pick a source to begin</span>;
  } else if (stage === "scanning") {
    const label =
      phase === "Done"
        ? "Scan complete"
        : phase === "Failed"
          ? "Scan failed"
          : phase === "Cancelled"
            ? "Scan cancelled"
            : `Scanning — ${phase ?? "starting"}`;
    dot = phase === "Failed" ? "err" : phase === "Done" ? "ok" : "live";
    left = (
      <span className="dw-status-item">
        {label}
        {scanDeep && phase !== "Done" ? " · deep mode" : ""}
      </span>
    );
  } else {
    dot = "ok";
    left = (
      <span className="dw-status-item">
        {formatCount(total || rows.length)} results · {formatCount(selected.size)} selected
        {selected.size > 0 &&
          ` · ${formatBytes(rows.filter((r) => selected.has(r.id)).reduce((a, r) => a + r.size_bytes, 0))}`}
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

/** Silent helper: pre-warm the disks list before the first paint of
 * the source view asks for it. */
export function warmDisks(): void {
  invoke<{ disks: unknown[] }>("list_disks", { req: {} })
    .then((res) => {
      const app = useAppStore.getState();
      const disks = (res.disks ?? []) as never[];
      app.setDisks(disks);
      app.setDisksLoading(false);
    })
    .catch((e) => {
      const app = useAppStore.getState();
      app.setDisksError(String(e));
      app.setDisksLoading(false);
    });
}
