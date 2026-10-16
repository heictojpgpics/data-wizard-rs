/**
 * Scanning view — the phase timeline, live progress with rate, and the
 * streaming "found files" feed. Terminal states render a summary panel
 * that hands off to the results view.
 */
import { useEffect, useRef, useState } from "react";
import { invoke } from "../lib/ipc";
import type { RecoveredFile, ScanProgress } from "../bindings";
import {
  ArrowRightIcon,
  CategoryGlyph,
  CheckIcon,
  CircleCheckIcon,
  CircleXIcon,
  RefreshIcon,
  ShieldCheckIcon,
  XIcon,
} from "../lib/Icon";
import { categoryOf, categoryVar, formatBytes, formatCount, formatRate } from "../lib/format";
import { useAppStore } from "../state/app";
import { useScanStore } from "../state/scan";

const PHASES: { key: ScanProgress["phase"]; label: string; hint: string }[] = [
  { key: "Probing", label: "Identify", hint: "Reading the volume boot record and picking the right engine" },
  { key: "QuickScan", label: "File records", hint: "Walking the filesystem's own metadata" },
  { key: "DeepScan", label: "Unallocated", hint: "Scanning free space cluster-by-cluster" },
  { key: "Carving", label: "Carving", hint: "Rebuilding files from raw signatures" },
  { key: "Finalizing", label: "Finishing", hint: "Building the results index" },
];

/** Quick scans skip the deep phases — map timeline accordingly. */
function phaseIndex(p: ScanProgress["phase"], deep: boolean): number {
  switch (p) {
    case "Probing":
      return 0;
    case "QuickScan":
      return 1;
    case "DeepScan":
      return 2;
    case "Carving":
      return deep ? 3 : 2;
    case "Finalizing":
      return deep ? 4 : 3;
    case "Done":
      return deep ? 5 : 4;
    case "Cancelled":
    case "Failed":
      return 0;
  }
}

export function ScanningView({ onViewResults }: { onViewResults: () => void }) {
  const scanId = useAppStore((s) => s.scanId);
  const deep = useAppStore((s) => s.scanDeep);
  const source = useAppStore((s) => s.source);
  const liveFeed = useAppStore((s) => s.liveFeed);
  const pushLiveFeed = useAppStore((s) => s.pushLiveFeed);
  const clearScan = useAppStore((s) => s.clearScan);
  const progress = useScanStore((s) => s.progress);
  const rate = useScanStore((s) => s.rate);
  const cancelScan = useScanStore((s) => s.cancelScan);

  const startedAt = useAppStore((s) => s.scanStartedAt);
  const [now, setNow] = useState(() => Date.now());
  const feedTimer = useRef<number | null>(null);

  // Clock for the elapsed display.
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(t);
  }, []);

  // Live "found" feed: poll the growing results while QuickScan runs
  // (the engine's own streaming path fills list_results as it finds).
  useEffect(() => {
    if (!scanId) return;
    const stopPhases: ScanProgress["phase"][] = ["Done", "Cancelled", "Failed", "Finalizing"];
    feedTimer.current = window.setInterval(async () => {
      const ph = useScanStore.getState().progress?.phase;
      if (!ph || stopPhases.includes(ph)) return;
      try {
        const res = await invoke<{ items: RecoveredFile[]; total: number }>("list_results", {
          req: { scan_id: scanId, offset: 0, limit: 8, extension_filter: null },
        });
        if (res.items && res.items.length > 0) {
          for (const f of res.items.slice(0, 8)) pushLiveFeed(f);
        }
      } catch {
        /* transient (spill tier warming) — next tick retries */
      }
    }, 420);
    return () => {
      if (feedTimer.current !== null) window.clearInterval(feedTimer.current);
    };
  }, [scanId, pushLiveFeed]);

  // Terminal hand-off: brief success beat, then results.
  useEffect(() => {
    if (progress?.phase === "Done") {
      const t = window.setTimeout(onViewResults, 650);
      return () => window.clearTimeout(t);
    }
  }, [progress?.phase, onViewResults]);

  if (!scanId) return null;

  const phase = progress?.phase ?? "Probing";
  const done = phase === "Done";
  const failed = phase === "Failed";
  const cancelled = phase === "Cancelled";
  const terminal = done || failed || cancelled;

  const activeIdx = terminal ? (done ? PHASES.length : -1) : phaseIndex(phase, deep);
  const visiblePhases = deep ? PHASES : PHASES.filter((p) => p.key !== "DeepScan");
  const pct =
    progress && progress.total > 0
      ? Math.min(100, (progress.current / progress.total) * 100)
      : 0;
  const found = progress?.current ?? 0;
  const elapsed = Math.max(0, (now - startedAt) / 1000);

  return (
    <div className="dw-scan-page">
      <div className={`dw-scan-card${terminal ? " dw-terminal" : ""}`}>
        {/* ── Header: source + mode + readonly badge ─────────────────── */}
        <div className="dw-scan-head">
          <div className="dw-scan-source">
            <span className="dw-scan-source-label">Scanning</span>
            <span className="dw-scan-source-name dw-selectable-text">{source?.label ?? "Source"}</span>
            <span className="dw-scan-source-detail">{source?.detail}</span>
          </div>
          <div className="dw-scan-badges">
            <span className={`dw-mode-badge${deep ? " dw-deep" : ""}`}>
              {deep ? "Deep scan" : "Quick scan"}
            </span>
            <span className="dw-readonly-inline" title="The scan only reads from this source">
              <ShieldCheckIcon size={13} />
              Read-only
            </span>
          </div>
        </div>

        {/* ── The orb: pulsing while working, resolved at terminal ────── */}
        <div className="dw-scan-orb-zone">
          <div className={`dw-scan-orb${terminal ? " dw-done" : ""}${failed ? " dw-failed" : ""}`}>
            {!terminal && <span className="dw-scan-orb-ring" aria-hidden />}
            <span className="dw-scan-orb-icon">
              {done ? (
                <CheckIcon size={26} />
              ) : failed ? (
                <XIcon size={26} />
              ) : cancelled ? (
                <XIcon size={26} />
              ) : (
                <RefreshIcon size={24} className="dw-scan-orb-spin" />
              )}
            </span>
          </div>
          <div className="dw-scan-status-line">
            {done ? (
              <span className="dw-scan-status dw-ok">Scan complete</span>
            ) : failed ? (
              <span className="dw-scan-status dw-err">Scan failed</span>
            ) : cancelled ? (
              <span className="dw-scan-status dw-muted">Scan cancelled</span>
            ) : (
              <span className="dw-scan-status dw-live">
                <span className="dw-live-dot" aria-hidden />
                {PHASES.find((p) => p.key === phase)?.hint ?? "Working…"}
              </span>
            )}
            {progress?.message && !terminal && (
              <span className="dw-scan-message dw-selectable-text">{progress.message}</span>
            )}
            {failed && progress?.message && (
              <span className="dw-scan-message dw-err dw-selectable-text">{progress.message}</span>
            )}
          </div>
        </div>

        {/* ── Phase timeline ──────────────────────────────────────────── */}
        <ol className={`dw-timeline${terminal && !done ? " dw-muted" : ""}`}>
          {visiblePhases.map((p, i) => {
            const state =
              done || i < activeIdx ? "done" : i === activeIdx ? "active" : "todo";
            return (
              <li key={p.key} className={`dw-tl-step dw-${state}`} aria-current={state === "active" ? "step" : undefined}>
                <span className="dw-tl-dot" aria-hidden />
                <span className="dw-tl-label">{p.label}</span>
                <span className="dw-tl-bar" aria-hidden />
              </li>
            );
          })}
        </ol>

        {/* ── Progress + counters ─────────────────────────────────────── */}
        {!terminal && (
          <>
            <div className="dw-scan-progress">
              <div className="dw-track dw-lg">
                <div className="dw-track-fill dw-active" style={{ width: `${Math.max(2, pct)}%` }} />
              </div>
              <div className="dw-scan-progress-meta">
                <span className="dw-scan-progress-pct">{pct.toFixed(0)}%</span>
                {rate != null && <span className="dw-scan-progress-rate">{formatRate(rate)}</span>}
                <span className="dw-scan-progress-elapsed">{formatElapsed(elapsed)}</span>
              </div>
            </div>

            <div className="dw-scan-counters">
              <div className="dw-scan-counter">
                <span className="dw-scan-counter-num">{formatCount(found)}</span>
                <span className="dw-scan-counter-label">files found</span>
              </div>
              <div className="dw-scan-counter dw-quiet">
                <span className="dw-scan-counter-num">live</span>
                <span className="dw-scan-counter-label">results stream in below as they are found</span>
              </div>
            </div>

            {/* ── Live found feed ─────────────────────────────────────── */}
            <div className="dw-scan-feed" aria-live="polite">
              {liveFeed.length === 0 ? (
                <div className="dw-scan-feed-empty">
                  <div className="dw-skeleton dw-feed-skel" />
                  <div className="dw-skeleton dw-feed-skel" />
                  <div className="dw-skeleton dw-feed-skel" />
                </div>
              ) : (
                liveFeed.slice(0, 7).map((f, i) => {
                  const cat = categoryOf(f.extension);
                  return (
                    <div key={f.id} className="dw-feed-row" style={{ animationDelay: `${Math.min(i, 4) * 24}ms` }}>
                      <span className="dw-feed-glyph" style={{ color: categoryVar(cat) }}>
                        <CategoryGlyph cat={cat} size={14} />
                      </span>
                      <span className="dw-feed-name dw-selectable-text">{f.name}</span>
                      <span className="dw-feed-path dw-selectable-text">{f.parent_path}</span>
                      <span className="dw-feed-size">{formatBytes(f.size_bytes)}</span>
                    </div>
                  );
                })
              )}
            </div>

            <div className="dw-scan-actions">
              <button className="dw-btn-outline dw-danger" onClick={() => void cancelScan()}>
                <XIcon size={14} />
                Cancel scan
              </button>
              <span className="dw-scan-cancel-note">
                Partial results are kept — you can review whatever was found so far.
              </span>
            </div>
          </>
        )}

        {/* ── Terminal panels ─────────────────────────────────────────── */}
        {done && (
          <div className="dw-scan-summary dw-slide-up">
            <div className="dw-scan-summary-icon">
              <CircleCheckIcon size={22} />
            </div>
            <div className="dw-scan-summary-body">
              <h3>Found {formatCount(found)} files</h3>
              <p>Review them, filter by type, and recover what you need.</p>
            </div>
            <button className="dw-btn-ink dw-lg" onClick={onViewResults}>
              Review files
              <ArrowRightIcon size={15} />
            </button>
          </div>
        )}
        {(failed || cancelled) && (
          <div className="dw-scan-summary dw-muted-panel dw-slide-up">
            <div className="dw-scan-summary-icon dw-warn">
              <CircleXIcon size={22} />
            </div>
            <div className="dw-scan-summary-body">
              <h3>{failed ? "The scan could not finish" : "Scan cancelled"}</h3>
              <p>
                {failed
                  ? "The engine reported an error. Details above; nothing on the source was modified."
                  : `Partial results were kept${found > 0 ? ` — ${formatCount(found)} files were found before stopping` : ""}.`}
              </p>
            </div>
            {found > 0 ? (
              <button className="dw-btn-ink" onClick={onViewResults}>
                Review partial results
                <ArrowRightIcon size={14} />
              </button>
            ) : (
              <button className="dw-btn-outline" onClick={clearScan}>
                Back to sources
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function formatElapsed(secs: number): string {
  if (secs < 60) return `${Math.floor(secs)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}m ${String(s).padStart(2, "0")}s`;
}
