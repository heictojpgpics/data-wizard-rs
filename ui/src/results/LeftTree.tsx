/**
 * LeftTree — the results location tree (EaseUS CLeftTreeView parity).
 *
 *   root            the scanned source (name + live total)
 *   special groups  Deleted · Existing · Lost (RAW) · Recycle Bin,
 *                   each with a live count (the odometer feel)
 *   type rail       the 8 categories with counts (photos first)
 *   folders         (path view) the directory children of the current
 *                   location — the tree doubles as navigation
 *
 * Selection is the FILTER's `special` + the location's `category`/
 * `path` — one source of truth, no local state.
 */

import { useMemo } from "react";
import {
  ChevronRightIcon,
  FolderOpenIcon,
  HardDriveIcon,
  RecycleBinIcon,
  SparklesIcon,
  TrashIcon,
  LayersIcon,
  CategoryGlyph,
} from "../lib/Icon";
import {
  CATEGORY_ORDER_SAFE,
  SPECIAL_GROUP_META,
  type CategoryId,
  type SpecialGroup,
} from "../lib/format";
import { useResultsStore } from "../state/results";

const SPECIAL_ORDER: SpecialGroup[] = ["deleted", "existing", "lost", "recycled"];

function SpecialIcon({ group }: { group: SpecialGroup }) {
  switch (group) {
    case "deleted":
      return <TrashIcon size={15} />;
    case "existing":
      return <LayersIcon size={15} />;
    case "lost":
      return <SparklesIcon size={15} />;
    case "recycled":
      return <RecycleBinIcon size={15} />;
  }
}

export function LeftTree({
  sourceLabel,
  onOpenSpecial,
}: {
  sourceLabel: string;
  /** Notify the parent (which may switch the view mode). */
  onOpenSpecial?: () => void;
}) {
  const filter = useResultsStore((s) => s.filter);
  const location = useResultsStore((s) => s.location);
  const navigate = useResultsStore((s) => s.navigate);
  const setFilter = useResultsStore((s) => s.setFilter);
  const grpCount = useResultsStore((s) => s.grpCount);
  const catCount = useResultsStore((s) => s.catCount);
  const childDirs = useResultsStore((s) => s.childDirs);
  const total = useResultsStore((s) => s.total);

  /* Directory children of the current path (path view only). */
  const dirs = useMemo(() => {
    if (location.view !== "path") return [];
    const key = location.path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase() || "/";
    return [...(childDirs.get(key) ?? [])]
      .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))
      .slice(0, 60);
  }, [childDirs, location]);

  function selectSpecial(g: SpecialGroup | null) {
    setFilter({ special: g });
    if (g != null && location.view === "path") {
      navigate({ view: "type", path: "/", category: null });
      onOpenSpecial?.();
    }
  }

  function selectCategory(c: CategoryId | null) {
    setFilter({ special: null });
    if (location.view === "path") {
      navigate({ view: "type", path: "/", category: c });
      onOpenSpecial?.();
    } else {
      navigate({ view: location.view, path: location.path, category: c });
    }
  }

  function openDir(d: string) {
    navigate({ view: "path", path: d, category: null });
  }

  const rootActive =
    filter.special == null && location.category == null && location.view === "type";

  return (
    <aside className="dw-lefttree" aria-label="Locations">
      {/* Root: everything found */}
      <button
        className={`dw-lt-node dw-lt-root${rootActive ? " dw-active" : ""}`}
        onClick={() => {
          setFilter({ special: null });
          navigate({ view: "type", path: "/", category: null });
        }}
      >
        <span className="dw-lt-icon">
          <HardDriveIcon size={16} />
        </span>
        <span className="dw-lt-label dw-selectable-text">{sourceLabel}</span>
        <span className="dw-lt-count tnum">{total.toLocaleString()}</span>
      </button>

      {/* Special groups */}
      <div className="dw-lt-group-label">Recovery groups</div>
      {SPECIAL_ORDER.map((g) => {
        const count = grpCount.get(g) ?? 0;
        const active = filter.special === g;
        return (
          <button
            key={g}
            className={`dw-lt-node${active ? " dw-active" : ""}`}
            data-tip={SPECIAL_GROUP_META[g].hint}
            onClick={() => selectSpecial(active ? null : g)}
          >
            <span className="dw-lt-icon dw-lt-special" data-group={g}>
              <SpecialIcon group={g} />
            </span>
            <span className="dw-lt-label">{SPECIAL_GROUP_META[g].label}</span>
            {count > 0 && (
              <span className="dw-lt-count tnum dw-count-pop" key={`${g}-${count}`}>
                {count.toLocaleString()}
              </span>
            )}
          </button>
        );
      })}

      {/* Type rail */}
      <div className="dw-lt-group-label">File types</div>
      {CATEGORY_ORDER_SAFE.map((c) => {
        const count = catCount.get(c)?.files ?? 0;
        const active = filter.special == null && location.category === c;
        return (
          <button
            key={c}
            className={`dw-lt-node${active ? " dw-active" : ""}${count === 0 ? " dw-empty" : ""}`}
            onClick={() => selectCategory(active ? null : c)}
          >
            <span className="dw-lt-icon dw-lt-cat" data-cat={c}>
              <CategoryGlyph cat={c} size={15} />
            </span>
            <span className="dw-lt-label">{c[0].toUpperCase() + c.slice(1)}</span>
            <span className="dw-lt-count tnum dw-count-pop" key={`${c}-${count}`}>
              {count.toLocaleString()}
            </span>
          </button>
        );
      })}

      {/* Folders at the current location (path view) */}
      {location.view === "path" && dirs.length > 0 && (
        <>
          <div className="dw-lt-group-label">Folders here</div>
          {dirs.map((d) => (
            <button key={d} className="dw-lt-node" onClick={() => openDir(d)}>
              <span className="dw-lt-icon">
                <FolderOpenIcon size={15} />
              </span>
              <span className="dw-lt-label dw-selectable-text">
                {d.slice(d.lastIndexOf("/") + 1)}
              </span>
              <ChevronRightIcon size={12} className="dw-lt-go" />
            </button>
          ))}
        </>
      )}
    </aside>
  );
}
