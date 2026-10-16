/**
 * Contextual sidebar — the supporting column that changes with the
 * wizard stage (a device tree would fight the linear wizard flow, so
 * the sidebar carries what each stage actually needs):
 *
 *   source   → saved scans to resume, the read-only education card
 *   scanning → live scan context (source, phase, counters)
 *   results  → the vertical category rail + scan info + actions
 */
import { useEffect, useMemo, useState } from "react";
import { invoke } from "../lib/ipc";
import type { ScanStatsDto, SavedScanSummary } from "../bindings";
import {
  CategoryGlyph,
  ClockIcon,
  HistoryIcon,
  SaveIcon,
  RotateCcwIcon,
  ShieldCheckIcon,
  SparklesIcon,
} from "../lib/Icon";
import {
  CATEGORIES,
  categoryVar,
  formatBytes,
  formatCount,
  timeAgo,
  type CategoryId,
} from "../lib/format";
import { useAppStore } from "../state/app";
import { useScanStore } from "../state/scan";

export function Sidebar() {
  const stage = useAppStore((s) => s.stage);
  return (
    <aside className="dw-sidebar" aria-label="Context panel">
      {stage === "source" && <SourceSidebar />}
      {stage === "scanning" && <ScanSidebar />}
      {stage === "results" && <ResultsSidebar />}
    </aside>
  );
}

/* ═════════════════════════════════════════════════════════════════════
 * Source stage: saved scans + education
 * ═════════════════════════════════════════════════════════════════════ */

function SourceSidebar() {
  const saved = useAppStore((s) => s.savedScans);
  const loading = useAppStore((s) => s.savedLoading);

  useEffect(() => {
    let cancelled = false;
    invoke<{ scans: SavedScanSummary[] }>("list_saved_scans", { req: {} })
      .then((res) => {
        if (!cancelled) {
          useAppStore.getState().setSavedScans(res.scans ?? []);
          useAppStore.getState().setSavedLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) useAppStore.getState().setSavedLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <>
      <SidebarSection title="Saved scans" icon={<HistoryIcon size={13} />}>
        {loading ? (
          <div className="dw-skeleton dw-side-skel" />
        ) : saved.length === 0 ? (
          <p className="dw-side-note">
            Scans you save appear here, ready to resume without rescanning.
          </p>
        ) : (
          <ul className="dw-side-saved">
            {saved.map((s) => (
              <li key={s.scan_id}>
                <button
                  className="dw-side-saved-btn"
                  onClick={() => useAppStore.getState().setStage("source")}
                  title={`${s.source_uri} · ${s.mode}`}
                >
                  <span className="dw-side-saved-glyph">
                    <ClockIcon size={13} />
                  </span>
                  <span className="dw-side-saved-text">
                    <span className="dw-side-saved-name dw-selectable-text">
                      {s.source_uri.split(/[\\/]/).pop() ?? s.source_uri}
                    </span>
                    <span className="dw-side-saved-meta">
                      {s.mode === "deep" ? "Deep" : "Quick"} · {formatCount(s.entries)} files · {timeAgo(s.created_at)}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </SidebarSection>

      <SidebarSection title="Why it's safe" icon={<ShieldCheckIcon size={13} />}>
        <ul className="dw-side-safety">
          <li>
            <strong>Read-only scanning.</strong> The engines open sources with
            read sharing only — a scan can never write to your disk.
          </li>
          <li>
            <strong>No-clobber recovery.</strong> Recovered files are written
            as temp copies, flushed, then published atomically.
          </li>
          <li>
            <strong>Memory-safe core.</strong> The parsers are written in Rust
            and fuzzed continuously against malformed inputs.
          </li>
        </ul>
      </SidebarSection>

      <div className="dw-sidebar-foot">
        <div className="dw-side-free">
          <SparklesIcon size={13} />
          <div>
            <strong>Every feature is free.</strong> No paywalled recovery
            limits, ever — MIT / Apache-2.0.
          </div>
        </div>
      </div>
    </>
  );
}

/* ═════════════════════════════════════════════════════════════════════
 * Scanning stage: live context
 * ═════════════════════════════════════════════════════════════════════ */

const PHASE_LABELS: { key: string; label: string }[] = [
  { key: "Probing", label: "Identify volume" },
  { key: "QuickScan", label: "File records" },
  { key: "DeepScan", label: "Unallocated space" },
  { key: "Carving", label: "Carving" },
  { key: "Finalizing", label: "Finalizing" },
];

function ScanSidebar() {
  const source = useAppStore((s) => s.source);
  const deep = useAppStore((s) => s.scanDeep);
  const progress = useScanStore((s) => s.progress);
  const rate = useScanStore((s) => s.rate);

  const phase = progress?.phase ?? "Probing";
  const found = progress?.current ?? 0;
  const total = progress?.total ?? 0;
  const pct = total > 0 ? Math.min(100, (found / total) * 100) : 0;

  return (
    <>
      <SidebarSection title="Current scan" icon={<RotateCcwIcon size={13} />}>
        <div className="dw-side-scan">
          <span className="dw-side-scan-name dw-selectable-text">{source?.label ?? "Source"}</span>
          <span className="dw-side-scan-detail">{source?.detail}</span>
          <span className={`dw-mode-badge${deep ? " dw-deep" : ""}`}>
            {deep ? "Deep scan" : "Quick scan"}
          </span>
        </div>
        <div className="dw-side-progress">
          <div className="dw-track">
            <div className="dw-track-fill dw-active" style={{ width: `${Math.max(3, pct)}%` }} />
          </div>
          <div className="dw-side-progress-meta">
            <span>{pct.toFixed(0)}%</span>
            {rate != null && <span>{formatCount(Math.round(rate * 60))} files/min</span>}
          </div>
        </div>
      </SidebarSection>

      <SidebarSection title="Phases" icon={<ClockIcon size={13} />}>
        <ol className="dw-side-phases">
          {PHASE_LABELS.filter((p) => deep || p.key !== "DeepScan").map((p) => {
            const idx = PHASE_LABELS.findIndex((x) => x.key === p.key);
            const cur = PHASE_LABELS.findIndex((x) => x.key === phase);
            const state = phase === "Done" || idx < cur ? "done" : idx === cur ? "active" : "todo";
            return (
              <li key={p.key} className={`dw-side-phase dw-${state}`}>
                <span className="dw-side-phase-dot" aria-hidden />
                {p.label}
              </li>
            );
          })}
        </ol>
      </SidebarSection>
    </>
  );
}

/* ═════════════════════════════════════════════════════════════════════
 * Results stage: category rail + scan info + actions
 * ═════════════════════════════════════════════════════════════════════ */

function ResultsSidebar() {
  const scanId = useAppStore((s) => s.scanId);
  const source = useAppStore((s) => s.source);
  const deep = useAppStore((s) => s.scanDeep);
  const categoryFilter = useAppStore((s) => s.categoryFilter);
  const rows = useAppStore((s) => s.rows);
  const setToast = useAppStore((s) => s.setToast);
  const [stats, setStats] = useState<ScanStatsDto | null>(null);

  useEffect(() => {
    if (!scanId) return;
    let cancelled = false;
    invoke<{ stats: ScanStatsDto }>("scan_stats", { req: { scan_id: scanId } })
      .then((r) => {
        if (!cancelled) setStats(r.stats);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [scanId]);

  const counts = useMemo(() => {
    const m = new Map<string, { files: number; bytes: number }>();
    for (const c of stats?.by_category ?? []) m.set(c.category, { files: c.files, bytes: c.bytes });
    return m;
  }, [stats]);

  async function saveScan() {
    if (!scanId) return;
    try {
      const res = await invoke<{ saved: boolean; entries: number }>("save_scan", {
        req: { scan_id: scanId },
      });
      setToast({
        text: res.saved
          ? `Scan saved — ${formatCount(res.entries)} entries checkpointed.`
          : "Nothing to save.",
        kind: "ok",
      });
    } catch (e) {
      setToast({ text: `Could not save: ${String(e)}`, kind: "err" });
    }
  }

  return (
    <>
      <SidebarSection title="Categories" icon={<SparklesIcon size={13} />}>
        <div className="dw-side-cats" role="tablist" aria-label="Filter by category">
          <button
            role="tab"
            aria-selected={categoryFilter === "all"}
            className={`dw-side-cat${categoryFilter === "all" ? " dw-active" : ""}`}
            onClick={() => useAppStore.getState().setCategoryFilter("all")}
          >
            <span className="dw-side-cat-label">All files</span>
            <span className="dw-side-cat-count">
              {formatCount(stats?.total_files ?? rows.length)}
            </span>
          </button>
          {CATEGORIES.map((c) => {
            const n = counts.get(c.id);
            if (!n || n.files === 0) return null;
            return (
              <button
                key={c.id}
                role="tab"
                aria-selected={categoryFilter === c.id}
                className={`dw-side-cat${categoryFilter === c.id ? " dw-active" : ""}`}
                onClick={() => useAppStore.getState().setCategoryFilter(c.id as CategoryId)}
              >
                <span className="dw-side-cat-icon" style={{ color: categoryVar(c.id) }}>
                  <CategoryGlyph cat={c.id} size={14} />
                </span>
                <span className="dw-side-cat-label">{c.label}</span>
                <span className="dw-side-cat-count">{formatCount(n.files)}</span>
                <span className="dw-side-cat-bytes">{formatBytes(n.bytes)}</span>
              </button>
            );
          })}
        </div>
      </SidebarSection>

      <SidebarSection title="This scan" icon={<ClockIcon size={13} />}>
        <div className="dw-side-scan">
          <span className="dw-side-scan-name dw-selectable-text">{source?.label ?? "Source"}</span>
          <span className="dw-side-scan-detail">
            {deep ? "Deep scan" : "Quick scan"} ·{" "}
            {formatCount(stats?.total_files ?? rows.length)} files ·{" "}
            {formatBytes(stats?.total_bytes ?? 0)}
          </span>
        </div>
        <div className="dw-side-actions">
          <button className="dw-btn-outline dw-side-action" onClick={() => void saveScan()}>
            <SaveIcon size={14} />
            Save scan
          </button>
          <button
            className="dw-btn-outline dw-side-action"
            onClick={() => useAppStore.getState().clearScan()}
          >
            <RotateCcwIcon size={14} />
            New scan
          </button>
        </div>
      </SidebarSection>

      <div className="dw-sidebar-foot">
        <div className="dw-side-free">
          <ShieldCheckIcon size={13} />
          <div>Recovery writes only to the destination you choose.</div>
        </div>
      </div>
    </>
  );
}

/* ── Shared section chrome ────────────────────────────────────────────── */

function SidebarSection({
  title,
  icon,
  children,
}: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="dw-side-section">
      <div className="dw-side-section-head">
        <span className="dw-side-section-icon">{icon}</span>
        <span className="dw-side-section-title">{title}</span>
      </div>
      <div className="dw-side-section-body">{children}</div>
    </section>
  );
}
