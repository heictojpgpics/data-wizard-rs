/**
 * Toolbar — the results navigate bar (EaseUS CNavigateBar parity):
 * back/forward history, breadcrumb, search box, filter button (opens
 * the drawer), view toggle (list/grid), group-by menu, sort menu.
 */

import { useEffect, useRef, useState } from "react";
import {
  ArrowLeftIcon,
  ChevronRightIcon,
  FilterIcon,
  GridViewIcon,
  ListIcon,
  SearchIcon,
  SortIcon,
  XIcon,
  LayersIcon,
  CalendarIcon,
  FolderOpenIcon,
} from "../lib/Icon";
import { formatCount } from "../lib/format";
import {
  useResultsStore,
  filterActive,
  type GroupBy,
  type SortKey,
  type ViewMode,
} from "../state/results";
import { Menu, MenuItem, MenuLabel, MenuSep, useMenu } from "../components/ui/Menu";

export function Toolbar({ onOpenFilter }: { onOpenFilter: () => void }) {
  const location = useResultsStore((s) => s.location);
  const history = useResultsStore((s) => s.history);
  const historyIndex = useResultsStore((s) => s.historyIndex);
  const back = useResultsStore((s) => s.back);
  const forward = useResultsStore((s) => s.forward);
  const navigate = useResultsStore((s) => s.navigate);
  const filter = useResultsStore((s) => s.filter);
  const setFilter = useResultsStore((s) => s.setFilter);
  const sort = useResultsStore((s) => s.sort);
  const setSort = useResultsStore((s) => s.setSort);
  const group = useResultsStore((s) => s.group);
  const setGroup = useResultsStore((s) => s.setGroup);

  const [q, setQ] = useState(filter.search);
  const searchTimer = useRef<number | null>(null);
  const sortMenu = useMenu();
  const groupMenu = useMenu();

  /* Debounced search → store. */
  useEffect(() => {
    if (searchTimer.current != null) window.clearTimeout(searchTimer.current);
    searchTimer.current = window.setTimeout(() => setFilter({ search: q }), 180);
    return () => {
      if (searchTimer.current != null) window.clearTimeout(searchTimer.current);
    };
  }, [q, setFilter]);

  const nActive = filterActive(filter);
  const canBack = historyIndex > 0;
  const canForward = historyIndex < history.length - 1;

  /* Breadcrumb segments for path view. */
  const crumbs =
    location.view === "path"
      ? location.path.split("/").filter(Boolean)
      : [];

  function crumbTo(i: number): string {
    return "/" + crumbs.slice(0, i + 1).join("/");
  }

  function setView(v: ViewMode) {
    navigate({ view: v === "grid" ? "grid" : v, path: location.path, category: location.category });
  }

  const viewMode: ViewMode = location.view;

  return (
    <div className="dw-toolbar">
      <div className="dw-nav-history">
        <button
          className="dw-icon-btn"
          aria-label="Back"
          disabled={!canBack}
          onClick={back}
          data-tip="Back"
        >
          <ArrowLeftIcon size={16} />
        </button>
        <button
          className="dw-icon-btn"
          aria-label="Forward"
          disabled={!canForward}
          onClick={forward}
          data-tip="Forward"
        >
          <ChevronRightIcon size={16} />
        </button>
      </div>

      {/* Breadcrumb / location */}
      <nav className="dw-breadcrumb" aria-label="Location">
        {location.view === "path" ? (
          <>
            <button
              className="dw-crumb"
              onClick={() => navigate({ view: "path", path: "/", category: null })}
            >
              <FolderOpenIcon size={14} />
              Root
            </button>
            {crumbs.map((c, i) => (
              <span key={crumbTo(i)} className="dw-crumb-seg">
                <ChevronRightIcon size={12} className="dw-crumb-sep" />
                <button
                  className="dw-crumb dw-selectable-text"
                  onClick={() => navigate({ view: "path", path: crumbTo(i), category: null })}
                >
                  {c}
                </button>
              </span>
            ))}
          </>
        ) : filter.special ? (
          <span className="dw-crumb dw-crumb-static">
            <LayersIcon size={14} />
            {filter.special === "deleted"
              ? "Deleted files"
              : filter.special === "existing"
                ? "Existing files"
                : filter.special === "lost"
                  ? "Lost files (deep scan)"
                  : "Recycle Bin"}
          </span>
        ) : (
          <span className="dw-crumb dw-crumb-static">
            {location.category ? `${location.category[0].toUpperCase()}${location.category.slice(1)}` : "All files"}
            <ChevronRightIcon size={12} className="dw-crumb-sep" />
            <span className="dw-crumb-note">by type</span>
          </span>
        )}
      </nav>

      {/* Search */}
      <div className="dw-searchbox" role="search">
        <SearchIcon size={14} />
        <input
          className="dw-search-input"
          placeholder="Search files or folders…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          aria-label="Search results"
          spellCheck={false}
        />
        {q && (
          <button className="dw-search-clear" aria-label="Clear search" onClick={() => setQ("")}>
            <XIcon size={12} />
          </button>
        )}
      </div>

      <div className="dw-toolbar-spacer" />

      {/* Filter */}
      <button
        className={`dw-btn-outline dw-sm${nActive > 0 ? " dw-has-badge" : ""}`}
        onClick={onOpenFilter}
        aria-pressed={nActive > 0}
      >
        <FilterIcon size={14} />
        Filter
        {nActive > 0 && <span className="dw-badge tnum">{nActive}</span>}
      </button>

      {/* View toggle */}
      <div className="dw-seg" role="radiogroup" aria-label="View mode">
        <button
          type="button"
          role="radio"
          aria-checked={viewMode !== "grid"}
          className={`dw-seg-item ${viewMode !== "grid" ? "dw-active" : ""}`}
          data-tip="List view"
          onClick={() => setView(location.view === "grid" ? "type" : location.view === "type" ? "path" : "path")}
        >
          <ListIcon size={15} />
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={viewMode === "grid"}
          className={`dw-seg-item ${viewMode === "grid" ? "dw-active" : ""}`}
          data-tip="Grid view (thumbnails)"
          onClick={() => setView("grid")}
        >
          <GridViewIcon size={15} />
        </button>
      </div>

      {/* Group by */}
      <button
        className="dw-btn-ghost dw-sm"
        aria-haspopup="menu"
        aria-expanded={groupMenu.state.open}
        onClick={(e) => groupMenu.openFrom(e.currentTarget)}
      >
        <LayersIcon size={14} />
        Group
      </button>
      <Menu state={groupMenu.state} onClose={groupMenu.close} minWidth={170}>
        <MenuLabel>Group by</MenuLabel>
        <MenuItem label="Type" icon={<LayersIcon size={14} />} checked={group === "type"} onSelect={() => { setGroup("type" as GroupBy); groupMenu.close(); }} />
        <MenuItem label="Date modified" icon={<CalendarIcon size={14} />} checked={group === "date"} onSelect={() => { setGroup("date" as GroupBy); groupMenu.close(); }} />
        <MenuSep />
        <MenuItem label="No grouping" checked={group === null} onSelect={() => { setGroup(null); groupMenu.close(); }} />
      </Menu>

      {/* Sort */}
      <button
        className="dw-btn-ghost dw-sm"
        aria-haspopup="menu"
        aria-expanded={sortMenu.state.open}
        onClick={(e) => sortMenu.openFrom(e.currentTarget)}
      >
        <SortIcon size={14} />
        Sort
      </button>
      <Menu state={sortMenu.state} onClose={sortMenu.close} minWidth={190}>
        <MenuLabel>Sort by</MenuLabel>
        <MenuItem
          label="Name"
          checked={sort.key === "name"}
          onSelect={() => { setSort("name" as SortKey); sortMenu.close(); }}
        />
        <MenuItem
          label="Size"
          checked={sort.key === "size"}
          onSelect={() => { setSort("size" as SortKey); sortMenu.close(); }}
        />
        <MenuItem
          label="Date modified"
          checked={sort.key === "modified"}
          onSelect={() => { setSort("modified" as SortKey); sortMenu.close(); }}
        />
        <MenuSep />
        <MenuLabel>Direction</MenuLabel>
        <MenuItem
          label="Ascending"
          checked={sort.asc}
          onSelect={() => { setSort(sort.key, true); sortMenu.close(); }}
        />
        <MenuItem
          label="Descending"
          checked={!sort.asc}
          onSelect={() => { setSort(sort.key, false); sortMenu.close(); }}
        />
      </Menu>
    </div>
  );
}

/** The search no-results hint (used by the empty state). */
export function searchHint(q: string): string {
  return `No results for “${q}” — try a shorter keyword, check the spelling, or clear the filters.`;
}

export function resultCountLabel(n: number): string {
  return `${formatCount(n)} ${n === 1 ? "file" : "files"}`;
}
