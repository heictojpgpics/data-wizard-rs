/**
 * Results view — stats header, the category/status filter rail, search
 * + sort, a virtualized results table with selection, and the sticky
 * recovery bar. Pages stream from list_results (server-side extension
 * filter for categories; client sort within loaded pages).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "../lib/ipc";
import type { ScanStatsDto } from "../bindings";
import {
  ArrowLeftIcon,
  CategoryGlyph,
  CheckIcon,
  DownloadIcon,
  FileQuestionIcon,
  RotateCcwIcon,
  SaveIcon,
  SearchIcon,
  SlidersIcon,
} from "../lib/Icon";
import {
  categoryOf,
  categoryVar,
  extensionsOf,
  formatBytes,
  formatCount,
  formatDate,
  parseFsTime,
  statusLabel,
  statusVar,
  type CategoryId,
} from "../lib/format";
import { RESULTS_PAGE, useAppStore } from "../state/app";
import { fetchResultsPage } from "../state/scan";

/* ── Stat cards ────────────────────────────────────────────────────────── */

function StatsRow({ stats }: { stats: ScanStatsDto | null }) {
  const rows = useAppStore((s) => s.rows);
  const total = stats?.total_files ?? rows.length;
  const bytes = stats?.total_bytes ?? rows.reduce((a, r) => a + r.size_bytes, 0);
  const recoverable = (stats?.by_status ?? [])
    .filter((s) => s.status === "Intact" || s.status === "CarvedValidated")
    .reduce((a, s) => a + s.files, 0);
  const damaged = (stats?.by_status ?? [])
    .filter((s) => s.status === "Overwritten" || s.status === "PartiallyRecovered")
    .reduce((a, s) => a + s.files, 0);
  const cats = (stats?.by_category ?? []).length;

  const cards = [
    { label: "Files found", value: formatCount(total), tone: "plain" as const },
    { label: "Recoverable data", value: formatBytes(bytes), tone: "ink" as const },
    { label: "Healthy / verified", value: formatCount(recoverable), tone: "ok" as const },
    { label: "Damaged / overwritten", value: formatCount(damaged), tone: "warn" as const },
    { label: "Categories", value: String(cats), tone: "plain" as const },
  ];
  return (
    <div className="dw-stats-row">
      {cards.map((c) => (
        <div key={c.label} className={`dw-stat-card dw-${c.tone}`}>
          <span className="dw-stat-value">{c.value}</span>
          <span className="dw-stat-label">{c.label}</span>
        </div>
      ))}
    </div>
  );
}

/* ── Results table ─────────────────────────────────────────────────────── */

function ResultsTable() {
  const scanId = useAppStore((s) => s.scanId);
  const rows = useAppStore((s) => s.rows);
  const total = useAppStore((s) => s.total);
  const categoryFilter = useAppStore((s) => s.categoryFilter);
  const search = useAppStore((s) => s.search);
  const sort = useAppStore((s) => s.sort);
  const sortAsc = useAppStore((s) => s.sortAsc);
  const selected = useAppStore((s) => s.selected);
  const toggleSelected = useAppStore((s) => s.toggleSelected);
  const setSelectedMany = useAppStore((s) => s.setSelectedMany);
  const setInspectedId = useAppStore((s) => s.setInspectedId);
  const inspectedId = useAppStore((s) => s.inspectedId);
  const loadingPage = useAppStore((s) => s.loadingPage);
  const exhausted = useAppStore((s) => s.exhausted);

  const parentRef = useRef<HTMLDivElement>(null);
  const loadingRef = useRef(false);

  // Sort + search the loaded window (server order is store order).
  const view = useMemo(() => {
    const q = search.trim().toLowerCase();
    let out = rows;
    if (q) {
      out = out.filter(
        (r) =>
          r.name.toLowerCase().includes(q) ||
          r.parent_path.toLowerCase().includes(q),
      );
    }
    const dir = sortAsc ? 1 : -1;
    const keyed = [...out];
    keyed.sort((a, b) => {
      switch (sort) {
        case "name":
          return a.name.localeCompare(b.name, undefined, { numeric: true }) * dir;
        case "modified": {
          const ta = parseFsTime(a.mtime)?.getTime() ?? 0;
          const tb = parseFsTime(b.mtime)?.getTime() ?? 0;
          return (ta - tb) * dir;
        }
        case "status":
          return a.status.localeCompare(b.status) * dir;
        case "size":
        default:
          return (a.size_bytes - b.size_bytes) * dir;
      }
    });
    return keyed;
  }, [rows, search, sort, sortAsc]);

  // eslint-disable-next-line react-hooks/incompatible-library -- @tanstack/react-virtual's API returns mutable handles by design; the React Compiler skip is expected and safe here.
  const virtualizer = useVirtualizer({
    count: view.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 40,
    overscan: 14,
  });

  // Infinite scroll: fetch the next page when near the end.
  useEffect(() => {
    const el = parentRef.current;
    if (!el || !scanId) return;
    const onScroll = () => {
      if (loadingRef.current || exhausted) return;
      const nearEnd = el.scrollTop + el.clientHeight >= el.scrollHeight - 480;
      if (nearEnd && rows.length < total) {
        loadingRef.current = true;
        const exts = categoryFilter === "all" ? null : extensionsOf(categoryFilter);
        useAppStore.getState().setLoadingPage(true);
        fetchResultsPage(scanId, rows.length, RESULTS_PAGE, exts)
          .then((res) => {
            useAppStore.getState().appendRows(res.items, res.total, res.spilled, false);
          })
          .catch(() => useAppStore.getState().setExhausted(true))
          .finally(() => {
            loadingRef.current = false;
            useAppStore.getState().setLoadingPage(false);
          });
      }
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, [scanId, rows.length, total, exhausted, categoryFilter]);

  const allShownSelected = view.length > 0 && view.every((r) => selected.has(r.id));

  return (
    <div className="dw-table-zone">
      <div className="dw-table-head">
        <span className="dw-th dw-th-check">
          <input
            type="checkbox"
            className="dw-check"
            aria-label="Select all shown rows"
            checked={allShownSelected}
            ref={(el) => {
              if (el) el.indeterminate = !allShownSelected && view.some((r) => selected.has(r.id));
            }}
            onChange={() => setSelectedMany(view.map((r) => r.id), !allShownSelected)}
          />
        </span>
        <button className="dw-th dw-th-name" onClick={() => useAppStore.getState().setSort("name")}>
          Name {sort === "name" && <SortMark asc={sortAsc} />}
        </button>
        <span className="dw-th dw-th-path">Folder</span>
        <button className="dw-th dw-th-size" onClick={() => useAppStore.getState().setSort("size")}>
          Size {sort === "size" && <SortMark asc={sortAsc} />}
        </button>
        <button className="dw-th dw-th-date" onClick={() => useAppStore.getState().setSort("modified")}>
          Modified {sort === "modified" && <SortMark asc={sortAsc} />}
        </button>
        <button className="dw-th dw-th-status" onClick={() => useAppStore.getState().setSort("status")}>
          Status {sort === "status" && <SortMark asc={sortAsc} />}
        </button>
        <span className="dw-th dw-th-conf">Confidence</span>
      </div>

      <div className="dw-table-scroll" ref={parentRef}>
        {view.length === 0 ? (
          <div className="dw-table-empty">
            <FileQuestionIcon size={28} />
            <p className="dw-table-empty-title">Nothing matches these filters</p>
            <p className="dw-table-empty-sub">
              Try clearing the search or picking a different category. A deep
              scan finds more in unallocated space.
            </p>
          </div>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((v) => {
              const r = view[v.index];
              const cat = categoryOf(r.extension);
              const isSel = selected.has(r.id);
              const isInspected = inspectedId === r.id;
              return (
                <div
                  key={r.id}
                  className={`dw-tr${isSel ? " dw-sel" : ""}${isInspected ? " dw-inspected" : ""}`}
                  style={{ position: "absolute", top: v.start, height: v.size, width: "100%" }}
                  onClick={() => setInspectedId(isInspected ? null : r.id)}
                >
                  <span className="dw-td dw-td-check" onClick={(e) => e.stopPropagation()}>
                    <input
                      type="checkbox"
                      className="dw-check"
                      aria-label={`Select ${r.name}`}
                      checked={isSel}
                      onChange={() => toggleSelected(r.id)}
                    />
                  </span>
                  <span className="dw-td dw-td-name">
                    <span className="dw-row-glyph" style={{ color: categoryVar(cat) }}>
                      <CategoryGlyph cat={cat} size={15} />
                    </span>
                    <span className="dw-row-name dw-selectable-text" title={r.name}>{r.name}</span>
                    {r.was_deleted && <span className="dw-row-deleted" title="Deleted on the source">deleted</span>}
                  </span>
                  <span className="dw-td dw-td-path dw-selectable-text" title={r.parent_path}>
                    {r.parent_path}
                  </span>
                  <span className="dw-td dw-td-size">{formatBytes(r.size_bytes)}</span>
                  <span className="dw-td dw-td-date">{formatDate(parseFsTime(r.mtime))}</span>
                  <span className="dw-td dw-td-status">
                    <span className="dw-status-pill" style={{ color: statusVar(r.status), borderColor: statusVar(r.status) }}>
                      {statusLabel(r.status)}
                    </span>
                  </span>
                  <span className="dw-td dw-td-conf">
                    <span className="dw-conf-track">
                      <span
                        className="dw-conf-fill"
                        style={{ width: `${Math.max(3, r.confidence)}%`, background: statusVar(r.status) }}
                      />
                    </span>
                    <span className="dw-conf-num">{r.confidence}</span>
                  </span>
                </div>
              );
            })}
          </div>
        )}
        {loadingPage && (
          <div className="dw-table-loading">
            <span className="dw-spinner" />
            <span>Loading more results…</span>
          </div>
        )}
      </div>
    </div>
  );
}

function SortMark({ asc }: { asc: boolean }) {
  return (
    <span className="dw-sort-mark" aria-hidden>
      {asc ? "▲" : "▼"}
    </span>
  );
}

/* ── The view ──────────────────────────────────────────────────────────── */

export function ResultsView({ onBack }: { onBack: () => void }) {
  const scanId = useAppStore((s) => s.scanId);
  const source = useAppStore((s) => s.source);
  const deep = useAppStore((s) => s.scanDeep);
  const [statsDto, setStatsDto] = useState<ScanStatsDto | null>(null);
  const categoryFilter = useAppStore((s) => s.categoryFilter);
  const statusFilter = useAppStore((s) => s.statusFilter);
  const search = useAppStore((s) => s.search);
  const rows = useAppStore((s) => s.rows);
  const selected = useAppStore((s) => s.selected);
  const setRecoveryOpen = useAppStore((s) => s.setRecoveryOpen);
  const setToast = useAppStore((s) => s.setToast);
  const clearLiveFeed = useAppStore((s) => s.clearLiveFeed);
  const resetBrowsing = useAppStore((s) => s.resetBrowsing);
  const spilled = useAppStore((s) => s.spilled);

  async function loadCategory(id: string, cat: CategoryId | "all") {
    const exts = cat === "all" ? null : extensionsOf(cat);
    useAppStore.getState().setLoadingPage(true);
    try {
      const res = await fetchResultsPage(id, 0, RESULTS_PAGE, exts);
      useAppStore.getState().appendRows(res.items, res.total, res.spilled, true);
    } catch {
      useAppStore.getState().setExhausted(true);
    } finally {
      useAppStore.getState().setLoadingPage(false);
    }
  }

  // Load first page + stats when entering.
  useEffect(() => {
    if (!scanId) return;
    let cancelled = false;
    clearLiveFeed();
    resetBrowsing();
    (async () => {
      const [pageRes, statsRes] = await Promise.all([
        fetchResultsPage(scanId, 0, RESULTS_PAGE, null),
        invoke<{ stats: ScanStatsDto }>("scan_stats", { req: { scan_id: scanId } })
          .then((r) => r.stats)
          .catch(() => null),
      ]);
      if (cancelled) return;
      useAppStore.getState().appendRows(pageRes.items, pageRes.total, pageRes.spilled, true);
      setStatsDto(statsRes);
    })().catch(() => undefined);
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scanId]);

  // Reload the page when the category filter changes (skips the
  // initial "all" — already loaded by the mount effect above).
  useEffect(() => {
    if (!scanId) return;
    if (categoryFilter === "all" && rows.length > 0) return;
    void loadCategory(scanId, categoryFilter);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [categoryFilter]);

  const selectedRows = useMemo(
    () => rows.filter((r) => selected.has(r.id)),
    [rows, selected],
  );
  const selectedBytes = selectedRows.reduce((a, r) => a + r.size_bytes, 0);

  async function saveScan() {
    if (!scanId) return;
    try {
      const res = await invoke<{ saved: boolean; entries: number }>("save_scan", {
        req: { scan_id: scanId },
      });
      setToast({
        text: res.saved
          ? `Scan saved — ${formatCount(res.entries)} entries checkpointed for resume.`
          : "Nothing to save yet.",
        kind: "ok",
      });
    } catch (e) {
      setToast({ text: `Could not save the scan: ${String(e)}`, kind: "err" });
    }
  }

  if (!scanId) return null;

  return (
    <div className="dw-results-page">
      {/* ── Header band ─────────────────────────────────────────────── */}
      <div className="dw-results-head">
        <div className="dw-results-head-main">
          <button className="dw-btn-ghost dw-icon" aria-label="Back to sources" onClick={onBack}>
            <ArrowLeftIcon size={16} />
          </button>
          <div className="dw-results-title-block">
            <h1 className="dw-results-title">
              {formatCount(statsDto?.total_files ?? rows.length)} files found
            </h1>
            <p className="dw-results-sub dw-selectable-text">
              {source?.label ?? "Source"} · {deep ? "deep scan" : "quick scan"}
              {spilled > 0 && ` · ${formatCount(spilled)} spilled to disk`}
            </p>
          </div>
        </div>
        <div className="dw-results-head-actions">
          <button className="dw-btn-outline" onClick={() => void saveScan()}>
            <SaveIcon size={14} />
            Save scan
          </button>
          <button className="dw-btn-outline" onClick={onBack}>
            <RotateCcwIcon size={14} />
            New scan
          </button>
        </div>
      </div>

      <StatsRow stats={statsDto} />

      {/* ── Toolbar (categories live in the sidebar rail) ─────────────────────────────────────────────── */}
      <div className="dw-filter-bar">
        <div className="dw-filter-controls">
          <div className="dw-search-capsule">
            <SearchIcon size={14} />
            <input
              className="dw-search-input"
              type="text"
              placeholder="Search name or folder…"
              value={search}
              onChange={(e) => useAppStore.getState().setSearch(e.target.value)}
              aria-label="Search results"
            />
          </div>
          <div className="dw-select-capsule">
            <SlidersIcon size={14} />
            <select
              className="dw-select dw-filter-select"
              value={statusFilter ?? ""}
              onChange={(e) =>
                useAppStore.getState().setStatusFilter(e.target.value === "" ? null : e.target.value)
              }
              aria-label="Filter by status"
            >
              <option value="">All statuses</option>
              <option value="Intact">Intact</option>
              <option value="CarvedValidated">Carved, verified</option>
              <option value="Carved">Carved</option>
              <option value="PartiallyRecovered">Partial</option>
              <option value="Overwritten">Overwritten</option>
              <option value="PendingValidation">Unchecked</option>
            </select>
          </div>
        </div>
      </div>

      {/* ── Table ───────────────────────────────────────────────────── */}
      <ResultsTable />

      {/* ── Selection bar ───────────────────────────────────────────── */}
      <SelectionBar
        count={selectedRows.length}
        bytes={selectedBytes}
        onRecover={() => setRecoveryOpen(true)}
      />
    </div>
  );
}

function SelectionBar({
  count,
  bytes,
  onRecover,
}: {
  count: number;
  bytes: number;
  onRecover: () => void;
}) {
  const clear = useAppStore((s) => s.clearSelection);
  if (count === 0) return null;
  return (
    <div className="dw-selection-bar dw-slide-up" role="region" aria-label="Selection actions">
      <div className="dw-selection-info">
        <span className="dw-selection-count">
          <CheckIcon size={13} />
          {formatCount(count)} selected
        </span>
        <span className="dw-selection-bytes">{formatBytes(bytes)}</span>
        <button className="dw-btn-ghost" onClick={clear}>
          Clear
        </button>
      </div>
      <button className="dw-btn-ink dw-lg" onClick={onRecover}>
        <DownloadIcon size={15} />
        Recover to…
      </button>
    </div>
  );
}
