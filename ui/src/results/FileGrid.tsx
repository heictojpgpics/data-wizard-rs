/**
 * FileGrid — the thumbnail grid view (EaseUS AnalysisGridWidget's grid
 * mode). Photo-forward cards: type-tinted tile, name, size, hover
 * affordances. Virtualized the same way as the table; the column count
 * tracks the container via ResizeObserver.
 */

import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { formatBytes } from "../lib/format";
import { useResultsStore, buildRows, type RowMeta } from "../state/results";
import { TypeIcon } from "../components/ui/TypeIcon";
import { RecoverabilityBadge } from "../components/ui/RecoverabilityBadge";

const CARD_W = 148;
const CARD_H = 168;
const GAP = 12;

export const FileGrid = memo(function FileGrid({ onInspect }: { onInspect: (id: string) => void }) {
  const store = useResultsStore();
  const { byId, childFiles, childDirs, subtree, filter, sort, location, revision, all, selected } =
    store;

  const metas = useMemo<RowMeta[]>(() => {
    const rows = buildRows({
      byId,
      childFiles,
      childDirs,
      subtree,
      filter,
      sort,
      group: null,
      location,
      revision,
      all,
    });
    return rows.flatMap((r) => (r.kind === "file" ? [r.meta] : []));
  }, [byId, childFiles, childDirs, subtree, filter, sort, location, revision, all]);

  const parentRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = parentRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? 0;
      setWidth((prev) => (prev === w ? prev : w));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const cols = Math.max(1, Math.floor((width - GAP) / (CARD_W + GAP)));

  const virtualizer = useVirtualizer({
    count: Math.ceil(metas.length / cols),
    getScrollElement: () => parentRef.current,
    estimateSize: () => CARD_H + GAP,
    overscan: 4,
  });

  return (
    <div className="dw-file-grid-wrap" ref={parentRef}>
      <div style={{ height: virtualizer.getTotalSize(), position: "relative", width: "100%" }}>
        {virtualizer.getVirtualItems().map((v) => {
          const slice = metas.slice(v.index * cols, (v.index + 1) * cols);
          return (
            <div
              key={v.key}
              style={{
                position: "absolute",
                top: v.start,
                left: 0,
                right: 0,
                height: CARD_H,
                display: "grid",
                gridTemplateColumns: `repeat(${cols}, ${CARD_W}px)`,
                gap: GAP,
                justifyContent: "center",
              }}
            >
              {slice.map((m) => {
                const sel = selected.has(m.id);
                return (
                  <button
                    key={m.id}
                    className={`dw-fg-card${sel ? " dw-sel" : ""}`}
                    onClick={() => store.toggleSelected(m.id)}
                    onDoubleClick={() => onInspect(m.id)}
                    data-cat={m.cat}
                  >
                    <span className="dw-fg-tile">
                      <TypeIcon name={m.file.name} extension={m.file.extension} size={40} />
                      {sel && <span className="dw-fg-check" aria-hidden />}
                    </span>
                    <span className="dw-fg-name dw-selectable-text">{m.file.name}</span>
                    <span className="dw-fg-meta tnum">{formatBytes(m.file.size_bytes)}</span>
                    <RecoverabilityBadge
                      status={m.file.status}
                      confidence={m.file.confidence}
                      short
                    />
                  </button>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
});
