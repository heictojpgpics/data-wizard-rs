/**
 * ResultsView — the 3-pane results browser (EaseUS parity):
 *
 *   ┌───────────────┬────────────────────────────────────┐
 *   │  LeftTree     │ Toolbar (breadcrumb/search/…)      │
 *   │  (locations)  ├────────────────────────────────────┤
 *   │               │ FileTable / FileGrid               │
 *   │               │ (+ FilterDrawer overlay)           │
 *   ├───────────────┴────────────────────────────────────┤
 *   │ SelectionBar · FileInspector (right, optional)     │
 *   └─────────────────────────────────────────────────────┘
 *
 * Live during the deep scan: rows stream in (the scan pump), counters
 * pop, and a subtle "still scanning" indicator sits in the header.
 */

import { useMemo, useState } from "react";
import { DownloadIcon, ScanSearchIcon, XIcon, ClockIcon } from "../lib/Icon";
import { formatBytes } from "../lib/format";
import { useAppStore } from "../state/app";
import { useScanStore, formatEta } from "../state/scan";
import { useResultsStore, buildRows, selectionSummary, filterActive } from "../state/results";
import { useRecoveryStore } from "../state/recovery";
import { LeftTree } from "../results/LeftTree";
import { Toolbar, searchHint } from "../results/Toolbar";
import { FilterDrawer } from "../results/FilterDrawer";
import { FileTable } from "../results/FileTable";
import { FileGrid } from "../results/FileGrid";
import { EmptyState } from "../components/ui/EmptyState";
import { Odometer } from "../components/ui/Odometer";

export function ResultsView({ onInspect }: { onInspect: (id: string) => void }) {
  const source = useAppStore((s) => s.source);
  const scanPhase = useScanStore((s) => s.phase);
  const eta = useScanStore((s) => s.etaSec);

  const store = useResultsStore();
  const { byId, childFiles, childDirs, subtree, filter, sort, group, location, revision, all, selected } =
    store;

  const [filterOpen, setFilterOpen] = useState(false);
  const openRecovery = useRecoveryStore((s) => s.open);

  const rows = useMemo(
    () => buildRows({ byId, childFiles, childDirs, subtree, filter, sort, group, location, revision, all }),
    [byId, childFiles, childDirs, subtree, filter, sort, group, location, revision, all],
  );

  const fileCount = useMemo(
    () => rows.reduce((n, r) => (r.kind === "file" ? n + 1 : n), 0),
    [rows],
  );
  const sel = useMemo(() => selectionSummary(selected, byId), [selected, byId]);

  const scanning = scanPhase === "quick" || scanPhase === "deep" || scanPhase === "probing";
  const organizing = store.organizing;
  const noResultsAtAll = store.total === 0 && !scanning;

  return (
    <div className="dw-results">
      <LeftTree sourceLabel={source?.label ?? "Scan"} />

      <div className="dw-results-main">
        <Toolbar onOpenFilter={() => setFilterOpen((v) => !v)} />

        {/* Scan-in-progress ribbon */}
        {scanning && (
          <div className="dw-scan-ribbon" role="status">
            <span className="dw-scan-ribbon-dot" aria-hidden />
            <span>
              Deep scan still running — new files appear here live.{" "}
              <span className="tnum">{formatEta(eta)}</span>
            </span>
            <button className="dw-btn-ghost dw-sm" onClick={() => useAppStore.getState().setStage("scanning")}>
              <ScanSearchIcon size={13} />
              Scan view
            </button>
          </div>
        )}

        {/* Organizing overlay */}
        {organizing && (
          <div className="dw-organizing" role="status">
            <span className="dw-spinner" />
            <span>
              Organizing found files…
              {store.organizingPct != null && (
                <span className="tnum"> {Math.round(store.organizingPct * 100)}%</span>
              )}
            </span>
          </div>
        )}

        {/* The list */}
        <div className="dw-results-body">
          {noResultsAtAll ? (
            <EmptyState
              icon={<ScanSearchIcon size={30} />}
              title="Nothing was found on this source"
              body={
                <>
                  The filesystem records were unreadable and no signatures matched in unallocated
                  space. If the drive is failing, try scanning a{" "}
                  <b>disk image</b> of it instead — images can be read even when the original
                  errors out.
                </>
              }
              actions={
                <button
                  className="dw-btn-outline"
                  onClick={() => useAppStore.getState().clearScan()}
                >
                  Pick another location
                </button>
              }
            />
          ) : fileCount === 0 && filter.search.trim() ? (
            <EmptyState
              icon={<ScanSearchIcon size={28} />}
              title="No files match this search"
              body={searchHint(filter.search)}
              compact
              actions={
                <button
                  className="dw-btn-ghost"
                  onClick={() => store.setFilter({ search: "" })}
                >
                  Clear search
                </button>
              }
            />
          ) : fileCount === 0 && filterActive(filter) > 0 ? (
            <EmptyState
              icon={<ScanSearchIcon size={28} />}
              title="No files match the active filters"
              body="The filters are too narrow for what the scan found. Widen or reset them to see results again."
              compact
              actions={
                <button className="dw-btn-outline" onClick={() => store.clearFilter()}>
                  Reset filters
                </button>
              }
            />
          ) : fileCount === 0 && location.view === "path" ? (
            <EmptyState
              icon={<ClockIcon size={26} />}
              title="This folder is empty"
              body="Files in this scan lived elsewhere on the volume. Try another folder from the tree, or switch to the type view."
              compact
            />
          ) : location.view === "grid" ? (
            <FileGrid onInspect={onInspect} />
          ) : (
            <FileTable
              onInspect={onInspect}
              onNavigateDir={(p) =>
                store.navigate({ view: "path", path: p, category: null })
              }
            />
          )}

          <FilterDrawer open={filterOpen} onClose={() => setFilterOpen(false)} />
        </div>

        {/* Selection bar */}
        {sel.count > 0 && (
          <div className="dw-selbar dw-slide-up" role="status">
            <button
              className="dw-selbar-clear"
              aria-label="Clear selection"
              onClick={() => store.clearSelection()}
            >
              <XIcon size={13} />
            </button>
            <span className="dw-selbar-count tnum">
              Selected <Odometer value={sel.count} /> {sel.count === 1 ? "file" : "files"}
            </span>
            <span className="dw-selbar-bytes tnum">{formatBytes(sel.bytes)}</span>
            <span className="dw-selbar-note">
              {sel.files.some((m) => m.file.status === "Overwritten")
                ? "Overwritten files cannot be recovered — they're included for the record."
                : "Everything selected is recoverable."}
            </span>
            <button className="dw-btn-ink" onClick={() => openRecovery("selected")}>
              <DownloadIcon size={15} />
              Recover now
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
