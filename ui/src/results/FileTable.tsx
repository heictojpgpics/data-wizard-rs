/**
 * FileTable — the virtualized results list (EaseUS AnalysisGridWidget
 * role). Renders the flat Row[] model (files + group headers + folder
 * rows) through @tanstack/react-virtual at 60fps for 100k+ rows.
 *
 * Columns: checkbox · name (type icon) · recoverability · size · date.
 * Row affordances: hover checkbox, click = inspect, double-click =
 * preview, context menu (recover / copy path / toggle).
 */

import { memo, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { FolderIcon } from "../lib/Icon";
import { formatBytes, formatDate } from "../lib/format";
import {
  useResultsStore,
  buildRows,
  type Row,
} from "../state/results";
import { TypeIcon } from "../components/ui/TypeIcon";
import { RecoverabilityBadge } from "../components/ui/RecoverabilityBadge";
import { Menu, MenuItem, MenuSep, useMenu } from "../components/ui/Menu";
import { CopyIcon, DownloadIcon, EyeIcon, CheckIcon } from "../lib/Icon";
import { useRecoveryStore } from "../state/recovery";
import { useUiStore } from "../state/ui";
import { CATEGORY_ORDER_SAFE } from "../lib/format";
import { CategoryGlyph } from "../lib/Icon";

const ROW_H = 38;
const GROUP_H = 34;

export const FileTable = memo(function FileTable({
  onInspect,
  onNavigateDir,
}: {
  onInspect: (id: string) => void;
  onNavigateDir: (path: string) => void;
}) {
  const store = useResultsStore();
  const { filter, sort, group, location, revision, byId, selected, childFiles, childDirs, subtree, all } =
    store;

  const rows = useMemo<Row[]>(
    () => buildRows({ byId, childFiles, childDirs, subtree, filter, sort, group, location, revision, all }),
    [byId, childFiles, childDirs, subtree, filter, sort, group, location, revision, all],
  );

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => (rows[i].kind === "file" ? ROW_H : GROUP_H),
    overscan: 14,
  });

  const ctxMenu = useMenu();
  const [ctxRow, setCtxRow] = useState<Row | null>(null);
  const openRecovery = useRecoveryStore((s) => s.open);
  const toast = useUiStore((s) => s.pushToast);

  function onContextMenu(e: React.MouseEvent, row: Row) {
    if (row.kind !== "file") return;
    e.preventDefault();
    setCtxRow(row);
    ctxMenu.openAt(e.clientX, e.clientY);
  }

  function rowCheckbox(row: Row): {
    checked: boolean;
    toggle: () => void;
  } {
    if (row.kind !== "file") return { checked: false, toggle: () => {} };
    const id = row.meta.id;
    return {
      checked: selected.has(id),
      toggle: () => store.toggleSelected(id),
    };
  }

  return (
    <div className="dw-file-table-wrap">
      {/* Header */}
      <div className="dw-ft-header" role="row">
        <span className="dw-ft-col dw-ft-check">
          <input
            type="checkbox"
            className="dw-check"
            aria-label="Select all visible files"
            checked={visibleAllChecked(rows, selected)}
            ref={(el) => {
              if (el)
                el.indeterminate =
                  visibleSomeChecked(rows, selected) && !visibleAllChecked(rows, selected);
            }}
            onChange={() => {
              const metas = rows.filter((r): r is Extract<Row, { kind: "file" }> => r.kind === "file");
              store.selectVisible(
                metas.map((r) => r.meta),
                !visibleAllChecked(rows, selected),
              );
            }}
          />
        </span>
        <span className="dw-ft-col dw-ft-name">Name</span>
        <span className="dw-ft-col dw-ft-rec">Recoverability</span>
        <span className="dw-ft-col dw-ft-size">Size</span>
        <span className="dw-ft-col dw-ft-date">Modified</span>
      </div>

      {/* Virtualized body */}
      <div className="dw-ft-body" ref={parentRef}>
        <div
          style={{
            height: virtualizer.getTotalSize(),
            position: "relative",
            width: "100%",
          }}
        >
          {virtualizer.getVirtualItems().map((v) => {
            const row = rows[v.index];
            const top = v.start;
            if (row.kind === "group" || row.kind === "dir") {
              const isDir = row.kind === "dir";
              return (
                <div
                  key={v.key}
                  className={`dw-ft-grouprow${isDir ? " dw-dirrow" : ""}`}
                  style={{ position: "absolute", top, left: 0, right: 0, height: GROUP_H }}
                >
                  {isDir ? (
                    <button
                      className="dw-ft-dir"
                      onClick={() => onNavigateDir(row.header.path ?? "/")}
                    >
                      <FolderIcon size={15} />
                      <span className="dw-selectable-text">{row.header.label}</span>
                      <span className="dw-ft-dir-count tnum">
                        {row.header.count.toLocaleString()} files
                      </span>
                    </button>
                  ) : (
                    <>
                      <span className="dw-ft-group-icon">
                        {row.header.cat ? <CategoryGlyph cat={row.header.cat} size={14} /> : null}
                      </span>
                      <span className="dw-ft-group-label">{row.header.label}</span>
                      <span className="dw-ft-group-count tnum">
                        {row.header.count.toLocaleString()} · {formatBytes(row.header.bytes)}
                      </span>
                    </>
                  )}
                </div>
              );
            }

            const m = row.meta;
            const cb = rowCheckbox(row);
            const sel = selected.has(m.id);
            return (
              <div
                key={v.key}
                className={`dw-ft-row${sel ? " dw-sel" : ""}`}
                style={{ position: "absolute", top, left: 0, right: 0, height: ROW_H }}
                role="row"
                tabIndex={0}
                onClick={() => onInspect(m.id)}
                onDoubleClick={() => onInspect(m.id)}
                onKeyDown={(e) => {
                  if (e.key === " " || e.key === "Enter") {
                    e.preventDefault();
                    cb.toggle();
                  }
                }}
                onContextMenu={(e) => onContextMenu(e, row)}
              >
                <span className="dw-ft-col dw-ft-check">
                  <input
                    type="checkbox"
                    className="dw-check"
                    checked={cb.checked}
                    onChange={cb.toggle}
                    aria-label={`Select ${m.file.name}`}
                    onClick={(e) => e.stopPropagation()}
                  />
                </span>
                <span className="dw-ft-col dw-ft-name">
                  <TypeIcon name={m.file.name} extension={m.file.extension} size={22} />
                  <span className="dw-ft-name-text dw-selectable-text">{m.file.name}</span>
                  {m.group === "recycled" && <span className="dw-tag dw-tag-blue">Bin</span>}
                  {m.group === "lost" && <span className="dw-tag dw-tag-violet">RAW</span>}
                </span>
                <span className="dw-ft-col dw-ft-rec">
                  <RecoverabilityBadge status={m.file.status} confidence={m.file.confidence} />
                </span>
                <span className="dw-ft-col dw-ft-size tnum">{formatBytes(m.file.size_bytes)}</span>
                <span className="dw-ft-col dw-ft-date">{formatDate(m.mtime)}</span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Context menu */}
      <Menu state={ctxMenu.state} onClose={ctxMenu.close} minWidth={200}>
        {ctxRow?.kind === "file" && (
          <>
            <MenuItem
              icon={<EyeIcon size={14} />}
              label="Preview"
              onSelect={() => {
                onInspect(ctxRow.meta.id);
                ctxMenu.close();
              }}
            />
            <MenuItem
              icon={<DownloadIcon size={14} />}
              label={`Recover “${truncate(ctxRow.meta.file.name, 24)}”`}
              onSelect={() => {
                openRecovery([ctxRow.meta]);
                ctxMenu.close();
              }}
            />
            <MenuSep />
            <MenuItem
              icon={<CopyIcon size={14} />}
              label="Copy full path"
              onSelect={() => {
                void navigator.clipboard
                  ?.writeText(`${ctxRow.meta.file.parent_path}/${ctxRow.meta.file.name}`)
                  .then(() => toast({ kind: "ok", title: "Path copied" }))
                  .catch(() => undefined);
                ctxMenu.close();
              }}
            />
            <MenuItem
              icon={<CheckIcon size={14} />}
              label={selected.has(ctxRow.meta.id) ? "Deselect" : "Select"}
              onSelect={() => {
                store.toggleSelected(ctxRow.meta.id);
                ctxMenu.close();
              }}
            />
          </>
        )}
      </Menu>
    </div>
  );
});

function visibleAllChecked(rows: Row[], selected: Set<string>): boolean {
  const files = rows.filter((r): r is Extract<Row, { kind: "file" }> => r.kind === "file");
  return files.length > 0 && files.every((r) => selected.has(r.meta.id));
}

function visibleSomeChecked(rows: Row[], selected: Set<string>): boolean {
  return rows.some((r) => r.kind === "file" && selected.has(r.meta.id));
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

/* Re-export for the empty-state copy. */
export { CATEGORY_ORDER_SAFE };
