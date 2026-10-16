/**
 * Scanning view — the EaseUS ScanningView parity.
 *
 *   phase timeline     Probing → Quick scan → Deep scan (sector map)
 *   progress cluster   odometer (files found) + phase bar + rate/ETA/elapsed
 *   live feed          streaming found-file rows (from the results store,
 *                      which the scan pump keeps filling)
 *   category chips     live per-category counters (pop on update)
 *   controls           Pause/Resume · Stop (confirm) · Browse results now
 *   tip carousel       rotating reassurance copy (the EaseUS corpus)
 */

import { useEffect, useMemo, useState } from "react";
import {
  PauseIcon,
  PlayIcon,
  StopIcon,
  EyeIcon,
  ScanSearchIcon,
  ZapIcon,
  FileSearchIcon,
  CircleCheckIcon,
  CategoryGlyph,
} from "../lib/Icon";
import { formatBytes, CATEGORY_ORDER_SAFE } from "../lib/format";
import { useAppStore } from "../state/app";
import { useScanStore, formatClock, formatEta } from "../state/scan";
import { useResultsStore } from "../state/results";
import { useUiStore } from "../state/ui";
import { Odometer } from "../components/ui/Odometer";
import { TypeIcon } from "../components/ui/TypeIcon";
import { RecoverabilityBadge } from "../components/ui/RecoverabilityBadge";

const TIPS = [
  "Quick scan reads the filesystem's own records — deleted files usually appear within minutes.",
  "Files stream in while the scan runs. You can preview and recover them before it finishes.",
  "Deep scan reads every sector and rebuilds files by signature — the best odds for files lost long ago.",
  "It is highly probable your data is recoverable — most files survive deletion until overwritten.",
  "Nothing is written to the source drive at any point. Scanning is strictly read-only.",
];

function PhaseTimeline({ phase }: { phase: string }) {
  const steps = [
    { id: "probing", label: "Opening source" },
    { id: "quick", label: "Quick scan" },
    { id: "deep", label: "Deep scan" },
  ];
  const cur =
    phase === "probing" ? 0 : phase === "quick" ? 1 : phase === "quickComplete" || phase === "deep" ? 2 : 2;
  return (
    <div className="dw-phase-line" role="progressbar" aria-valuemin={0} aria-valuemax={3} aria-valuenow={cur}>
      {steps.map((s, i) => (
        <div
          key={s.id}
          className={`dw-phase-step${i < cur ? " dw-done" : ""}${i === cur ? " dw-current" : ""}`}
        >
          <span className="dw-phase-dot" aria-hidden />
          <span className="dw-phase-label">{s.label}</span>
          {i < steps.length - 1 && <span className="dw-phase-conn" aria-hidden />}
        </div>
      ))}
    </div>
  );
}

function TipCarousel() {
  const [i, setI] = useState(0);
  useEffect(() => {
    const t = window.setInterval(() => setI((v) => (v + 1) % TIPS.length), 9000);
    return () => window.clearInterval(t);
  }, []);
  return (
    <div className="dw-tip-carousel" aria-live="polite">
      <span className="dw-tip-bulb" aria-hidden>
        <ZapIcon size={14} />
      </span>
      <span key={i} className="dw-tip-text dw-fade-in">
        {TIPS[i]}
      </span>
    </div>
  );
}

export function ScanningView() {
  const app = useAppStore();
  const phase = useScanStore((s) => s.phase);
  const paused = useScanStore((s) => s.paused);
  const elapsed = useScanStore((s) => s.elapsedMs);
  const rate = useScanStore((s) => s.ratePerSec);
  const eta = useScanStore((s) => s.etaSec);
  const found = useScanStore((s) => s.found);
  const pause = useScanStore((s) => s.pause);
  const unpause = useScanStore((s) => s.unpause);
  const stop = useScanStore((s) => s.stop);
  const dismiss = useScanStore((s) => s.dismissQuickComplete);
  const openConfirm = useUiStore((s) => s.openConfirm);
  const setStage = useAppStore((s) => s.setStage);

  const all = useResultsStore((s) => s.all);
  const catCount = useResultsStore((s) => s.catCount);
  const revision = useResultsStore((s) => s.revision);

  /* The feed: the 30 most-recently-found files. */
  const feed = useMemo(() => all.slice(-30).reverse(), [all]);
  const cats = useMemo(
    () => CATEGORY_ORDER_SAFE.map((c) => ({ cat: c, count: catCount.get(c)?.files ?? 0 })),
    [catCount],
  );

  /* Phase-local progress (records walked / total from the engine). */
  const progress = useScanStore((s) => s.progress);
  const pct =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.current / progress.total) * 100))
      : null;

  const phaseLabel =
    phase === "probing"
      ? "Opening the source and identifying the filesystem…"
      : phase === "quick"
        ? "Quick scanning — reading the filesystem's records"
        : phase === "deep"
          ? "Deep scanning — sector-by-sector with signature carving"
          : phase === "quickComplete"
            ? "Quick scan complete — deep scan is finding more files"
            : "Finishing up — organizing what was found";

  if (phase === "quickComplete") {
    /* The interstitial: quick results are ready, deep is running. */
    return (
      <div className="dw-scan-page">
        <div className="dw-scan-hero dw-quickdone dw-pop-in">
          <span className="dw-quickdone-icon">
            <CircleCheckIcon size={34} />
          </span>
          <h1>Quick scan complete!</h1>
          <p>
            <b className="tnum">{found.toLocaleString()}</b> files are ready to browse right now.
            The deep scan keeps running in the background and streams in anything else it finds —
            files lost long ago, after a format, or with damaged records.
          </p>
          <div className="dw-quickdone-stats">
            {cats
              .filter((c) => c.count > 0)
              .slice(0, 6)
              .map((c) => (
                <span key={c.cat} className="dw-quickdone-stat">
                  <CategoryGlyph cat={c.cat} size={14} />
                  <span className="tnum">{c.count.toLocaleString()}</span>
                </span>
              ))}
          </div>
          <div className="dw-quickdone-actions">
            <button className="dw-btn-ink dw-lg" onClick={dismiss}>
              <EyeIcon size={16} />
              Browse results now
            </button>
            <span className="dw-quickdone-note">
              Deep scan continues while you browse — new files appear live.
            </span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="dw-scan-page">
      <div className="dw-scan-hero">
        <div className={`dw-scan-glyph${paused ? " dw-paused" : ""}`} aria-hidden>
          <ScanSearchIcon size={38} />
          <span className="dw-scan-ring" />
          <span className="dw-scan-ring dw-r2" />
        </div>
        <div className="dw-scan-title-block">
          <h1>
            {paused
              ? "Scan paused"
              : phase === "probing"
                ? "Reading the source…"
                : phase === "deep"
                  ? "Deep scanning"
                  : "Quick scanning"}
            <span className="dw-scan-count">
              <span className="dw-scan-count-label">Files found:</span>{" "}
              <Odometer value={found} className="dw-scan-odo" />
            </span>
          </h1>
          <p className="dw-scan-sub">{phaseLabel}</p>
          <PhaseTimeline phase={phase} />
        </div>
      </div>

      <div className="dw-scan-progress-card">
        <div className={`dw-track dw-lg${paused ? " dw-paused" : ""}`}>
          <div
            className="dw-track-fill dw-active"
            style={{ width: pct != null ? `${pct}%` : "100%" }}
          />
        </div>
        <div className="dw-scan-metrics tnum">
          <span>{pct != null ? `${pct}%` : "measuring…"}</span>
          <span>Elapsed {formatClock(elapsed)}</span>
          {rate != null && <span>{Math.round(rate * 60).toLocaleString()} files/min</span>}
          <span>{formatEta(eta)}</span>
        </div>
        <div className="dw-cat-chips" aria-label="Found by type">
          {cats.map((c) => (
            <span
              key={c.cat}
              className={`dw-cat-chip${c.count > 0 ? " dw-live" : ""}`}
              data-cat={c.cat}
            >
              <CategoryGlyph cat={c.cat} size={13} />
              <span className="dw-cat-chip-count tnum dw-count-pop" key={`${c.cat}-${c.count}`}>
                {c.count.toLocaleString()}
              </span>
            </span>
          ))}
        </div>
      </div>

      <div className="dw-scan-feed">
        <div className="dw-scan-feed-head">
          <FileSearchIcon size={14} />
          <span>Live findings</span>
          <span className="dw-scan-feed-note">
            Streaming as the scan finds them — preview and recovery stay open the whole time
          </span>
        </div>
        <div className="dw-scan-feed-list" key={revision}>
          {feed.length === 0 ? (
            <div className="dw-scan-feed-empty">
              First findings appear within seconds — the engine is reading the filesystem index
              right now.
            </div>
          ) : (
            feed.map((f) => (
              <div key={f.id} className="dw-scan-feed-row dw-row-in">
                <TypeIcon name={f.name} extension={f.extension} size={22} />
                <span className="dw-scan-feed-name dw-selectable-text">{f.name}</span>
                <span className="dw-scan-feed-path dw-selectable-text">{f.parent_path}</span>
                <span className="dw-scan-feed-size tnum">{formatBytes(f.size_bytes)}</span>
                <RecoverabilityBadge status={f.status} confidence={f.confidence} short />
              </div>
            ))
          )}
        </div>
      </div>

      <div className="dw-scan-controls">
        <div className="dw-scan-controls-left">
          {paused ? (
            <button className="dw-btn-ink" onClick={() => void unpause()}>
              <PlayIcon size={15} />
              Resume
            </button>
          ) : (
            <button className="dw-btn-outline" onClick={() => void pause()}>
              <PauseIcon size={15} />
              Pause
            </button>
          )}
          <button
            className="dw-btn-outline dw-danger"
            onClick={() =>
              openConfirm({
                title: "Stop searching for lost data?",
                body: (
                  <>
                    Files already found stay available for recovery, but anything not yet scanned
                    will not be found by this run. Stopping is safe — nothing is half-written.
                  </>
                ),
                confirmLabel: "Stop scan",
                cancelLabel: "Keep scanning",
                tone: "warn",
                onConfirm: () => void stop(),
              })
            }
          >
            <StopIcon size={14} />
            Stop
          </button>
        </div>
        <TipCarousel />
        <div className="dw-scan-controls-right">
          {app.scanDeep && phase === "quick" && (
            <button className="dw-btn-ghost" onClick={() => setStage("results")}>
              <EyeIcon size={15} />
              Peek at results
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/* Small local helper kept next to its only consumer. */
export function scanViewFooterHint(): string {
  return "Scanning is read-only. Recovered files are always written to a different drive you pick.";
}
