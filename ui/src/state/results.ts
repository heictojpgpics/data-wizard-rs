/**
 * Results store — the EaseUS-parity browsing engine.
 *
 * Owns the FULL in-memory result set plus the indexes that make
 * 100k+ rows navigable at 60fps:
 *
 *   byId        id → row                       (O(1) selection lookups)
 *   childFiles  parentPath → file ids          (path view navigation)
 *   childDirs   parentPath → subdirectory set  (folder rows)
 *   subtree     path → {files, bytes}          (folder row counts)
 *   counters    category / special-group /     (live odometers during
 *               status / date-bucket totals     the scan, no O(n) passes)
 *
 * Every mutation flows through `ingest`/`reset` and bumps `revision`;
 * derived selectors (visible rows, group headers) are memoized against
 * revision + the filter/sort/browse state.
 */

import { create } from "zustand";
import type { RecoveredFile } from "../bindings";
import {
  categoryOf,
  dateBucketOf,
  isJunkFile,
  isSystemPath,
  parseFsTime,
  specialGroupOf,
  type CategoryId,
  type SpecialGroup,
} from "../lib/format";

/* ── Path normalization ────────────────────────────────────────────────── */

export function normPath(p: string): string {
  const s = p.replace(/\\/g, "/").replace(/\/+$/, "");
  return s.length === 0 ? "/" : s;
}

const keyOf = (p: string): string => normPath(p).toLowerCase();

/** Parent path of a normalized path ("/a/b" → "/a"; "/a" → "/"). */
export function parentOf(p: string): string {
  const s = normPath(p);
  const i = s.lastIndexOf("/");
  return i <= 0 ? "/" : s.slice(0, i);
}

/* ── Filter model (the filter drawer) ──────────────────────────────────── */

export interface ResultsFilter {
  /** Left-tree special group (null = none selected). */
  special: SpecialGroup | null;
  /** Selected categories (empty = all). */
  categories: Set<CategoryId>;
  /** Selected size-range ids (empty = all). */
  sizeRanges: Set<string>;
  /** Date-range id from DATE_RANGES, or "custom". */
  dateRange: string | null;
  /** Custom range epoch ms (inclusive). */
  dateFrom: number | null;
  dateTo: number | null;
  /** Raw extension query ("*.doc; *.xlsx"). */
  extensions: string;
  /** Search box (name or path substring). */
  search: string;
  deletedOnly: boolean;
  hideJunk: boolean;
  hideSystem: boolean;
  /** Hide overwritten (unrecoverable) entries. */
  hideOverwritten: boolean;
}

export const EMPTY_FILTER: ResultsFilter = {
  special: null,
  categories: new Set(),
  sizeRanges: new Set(),
  dateRange: null,
  dateFrom: null,
  dateTo: null,
  extensions: "",
  search: "",
  deletedOnly: false,
  hideJunk: false,
  hideSystem: false,
  hideOverwritten: false,
};

export function filterActive(f: ResultsFilter): number {
  let n = 0;
  if (f.special) n++;
  if (f.categories.size) n++;
  if (f.sizeRanges.size) n++;
  if (f.dateRange || f.dateFrom != null || f.dateTo != null) n++;
  if (f.extensions.trim()) n++;
  if (f.search.trim()) n++;
  if (f.deletedOnly) n++;
  if (f.hideJunk) n++;
  if (f.hideSystem) n++;
  if (f.hideOverwritten) n++;
  return n;
}

/* ── Sort / group / browse ─────────────────────────────────────────────── */

export type SortKey = "name" | "size" | "modified";
export type GroupBy = "type" | "date" | null;
export type ViewMode = "path" | "type" | "grid";

export interface BrowseLocation {
  view: ViewMode;
  /** Path view: the folder being viewed. */
  path: string;
  /** Type view: the selected category (null = all). */
  category: CategoryId | null;
}

export const ROOT_LOCATION: BrowseLocation = { view: "type", path: "/", category: null };

/* ── Precomputed per-row view model (kept lean for 100k rows) ──────────── */

export interface RowMeta {
  id: string;
  file: RecoveredFile;
  cat: CategoryId;
  group: SpecialGroup;
  mtime: Date | null;
}

export interface GroupHeader {
  key: string;
  label: string;
  count: number;
  bytes: number;
  cat?: CategoryId;
  kind: "type" | "date" | "dir";
  /** Directory rows in path view. */
  path?: string;
}

export type Row =
  | { kind: "file"; meta: RowMeta }
  | { kind: "dir"; header: GroupHeader }
  | { kind: "group"; header: GroupHeader };

/* ── Store ─────────────────────────────────────────────────────────────── */

interface ResultsStore {
  /* dataset */
  all: RecoveredFile[];
  revision: number;
  total: number; // server-reported total (may exceed loaded during scan)
  spilled: number;
  loadedAll: boolean;
  organizing: boolean;
  organizingPct: number | null;

  /* indexes (rebuilt on reset; incrementally maintained on ingest) */
  byId: Map<string, RowMeta>;
  childFiles: Map<string, string[]>;
  childDirs: Map<string, Set<string>>;
  subtree: Map<string, { files: number; bytes: number }>;
  catCount: Map<CategoryId, { files: number; bytes: number }>;
  grpCount: Map<SpecialGroup, number>;
  statusCount: Map<string, number>;
  dateBuckets: Map<string, number>;

  /* browsing state */
  filter: ResultsFilter;
  sort: { key: SortKey; asc: boolean };
  group: GroupBy;
  location: BrowseLocation;
  history: BrowseLocation[];
  historyIndex: number;

  /* selection */
  selected: Set<string>;

  /* actions */
  reset: (total: number) => void;
  setTotal: (total: number, spilled: number) => void;
  ingest: (items: RecoveredFile[]) => void;
  setOrganizing: (on: boolean, pct?: number | null) => void;
  setLoadedAll: (v: boolean) => void;

  setFilter: (patch: Partial<ResultsFilter>) => void;
  clearFilter: () => void;
  setSort: (key: SortKey, asc?: boolean) => void;
  setGroup: (g: GroupBy) => void;
  navigate: (loc: BrowseLocation) => void;
  back: () => void;
  forward: () => void;
  canBack: () => boolean;
  canForward: () => boolean;

  toggleSelected: (id: string) => void;
  selectMany: (ids: string[], on: boolean) => void;
  selectVisible: (metas: RowMeta[], on: boolean) => void;
  clearSelection: () => void;
}

function emptyMeta(file: RecoveredFile): RowMeta {
  return {
    id: file.id,
    file,
    cat: categoryOf(file.extension),
    group: specialGroupOf(file),
    mtime: parseFsTime(file.mtime),
  };
}

/** SIZE_RANGES / DATE_RANGES are imported lazily in predicates to avoid
 * a cycle with format.ts (they are plain data). */
import { DATE_RANGES, SIZE_RANGES } from "../lib/format";

export const useResultsStore = create<ResultsStore>((set, get) => ({
  all: [],
  revision: 0,
  total: 0,
  spilled: 0,
  loadedAll: false,
  organizing: false,
  organizingPct: null,

  byId: new Map(),
  childFiles: new Map(),
  childDirs: new Map(),
  subtree: new Map(),
  catCount: new Map(),
  grpCount: new Map(),
  statusCount: new Map(),
  dateBuckets: new Map(),

  filter: { ...EMPTY_FILTER, categories: new Set(), sizeRanges: new Set() },
  sort: { key: "size", asc: false },
  group: "type",
  location: ROOT_LOCATION,
  history: [ROOT_LOCATION],
  historyIndex: 0,

  selected: new Set<string>(),

  reset: (total) => {
    set({
      all: [],
      revision: get().revision + 1,
      total,
      spilled: 0,
      loadedAll: false,
      organizing: false,
      organizingPct: null,
      byId: new Map(),
      childFiles: new Map(),
      childDirs: new Map(),
      subtree: new Map(),
      catCount: new Map(),
      grpCount: new Map(),
      statusCount: new Map(),
      dateBuckets: new Map(),
      filter: { ...EMPTY_FILTER, categories: new Set(), sizeRanges: new Set() },
      sort: { key: "size", asc: false },
      group: "type",
      location: ROOT_LOCATION,
      history: [ROOT_LOCATION],
      historyIndex: 0,
      selected: new Set<string>(),
    });
  },

  setTotal: (total, spilled) => set({ total, spilled }),

  ingest: (items) => {
    if (items.length === 0) return;
    const st = get();
    /* Copy-on-write for the index maps: zustand selectors compare by
     * reference, so in-place mutation would leave subscribers (the
     * category chips, the left-tree counts) rendering stale values.
     * The dir-keyed maps are small (bounded by unique paths); byId is
     * the only large one and its consumers are revision-keyed. */
    const byId = st.byId;
    const childFiles = st.childFiles;
    const childDirs = st.childDirs;
    const subtree = st.subtree;
    const catCount = st.catCount;
    const grpCount = st.grpCount;
    const statusCount = st.statusCount;
    const dateBuckets = st.dateBuckets;

    const fresh: RecoveredFile[] = [];
    for (const f of items) {
      if (byId.has(f.id)) continue;
      fresh.push(f);

      const meta = emptyMeta(f);
      byId.set(f.id, meta);

      // Path indexes.
      const parentKey = keyOf(f.parent_path);
      let files = childFiles.get(parentKey);
      if (!files) {
        files = [];
        childFiles.set(parentKey, files);
      }
      files.push(f.id);

      if (parentKey !== "/") {
        // Register EVERY ancestor as a child of its parent — the tree
        // must be navigable from the root even when no file lives in
        // the intermediate directories (the VLM pass caught the empty
        // root: only the direct parent was being linked before).
        let chain = parentKey;
        while (chain !== "/") {
          const grandKey = parentOf(chain);
          let dirs = childDirs.get(grandKey);
          if (!dirs) {
            dirs = new Set();
            childDirs.set(grandKey, dirs);
          }
          dirs.add(chain);

          // Bump subtree counters along the same chain.
          let sub = subtree.get(chain);
          if (!sub) {
            sub = { files: 0, bytes: 0 };
            subtree.set(chain, sub);
          }
          sub.files++;
          sub.bytes += f.size_bytes;

          chain = grandKey;
        }
      }

      // Counters.
      let cc = catCount.get(meta.cat);
      if (!cc) {
        cc = { files: 0, bytes: 0 };
        catCount.set(meta.cat, cc);
      }
      cc.files++;
      cc.bytes += f.size_bytes;
      grpCount.set(meta.group, (grpCount.get(meta.group) ?? 0) + 1);
      statusCount.set(f.status, (statusCount.get(f.status) ?? 0) + 1);
      const bucket = dateBucketOf(meta.mtime);
      dateBuckets.set(bucket.id, (dateBuckets.get(bucket.id) ?? 0) + 1);
    }

    set({
      all: [...st.all, ...fresh],
      revision: st.revision + 1,
      /* New references for every mutated map — see the copy-on-write
       * note above. Shallow clones: values keep their identity. */
      catCount: new Map(catCount),
      grpCount: new Map(grpCount),
      statusCount: new Map(statusCount),
      dateBuckets: new Map(dateBuckets),
      childFiles: new Map(childFiles),
      childDirs: new Map(childDirs),
      subtree: new Map(subtree),
    });
  },

  setOrganizing: (on, pct) => set({ organizing: on, organizingPct: pct ?? null }),
  setLoadedAll: (v) => set({ loadedAll: v }),

  setFilter: (patch) =>
    set((st) => ({
      filter: { ...st.filter, ...patch },
      selected: patchResetSelection(patch) ? new Set<string>() : st.selected,
    })),

  clearFilter: () =>
    set({
      filter: { ...EMPTY_FILTER, categories: new Set(), sizeRanges: new Set() },
      selected: new Set<string>(),
    }),

  setSort: (key, asc) =>
    set((st) => ({
      sort: { key, asc: asc ?? (key === st.sort.key ? !st.sort.asc : key === "name") },
    })),

  setGroup: (g) => set({ group: g }),

  navigate: (loc) =>
    set((st) => {
      const history = st.history.slice(0, st.historyIndex + 1);
      history.push(loc);
      return { location: loc, history, historyIndex: history.length - 1 };
    }),

  back: () =>
    set((st) =>
      st.historyIndex > 0 ? { historyIndex: st.historyIndex - 1, location: st.history[st.historyIndex - 1] } : {},
    ),

  forward: () =>
    set((st) =>
      st.historyIndex < st.history.length - 1
        ? { historyIndex: st.historyIndex + 1, location: st.history[st.historyIndex + 1] }
        : {},
    ),

  canBack: () => get().historyIndex > 0,
  canForward: () => get().historyIndex < get().history.length - 1,

  toggleSelected: (id) =>
    set((st) => {
      const next = new Set(st.selected);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { selected: next };
    }),

  selectMany: (ids, on) =>
    set((st) => {
      const next = new Set(st.selected);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return { selected: next };
    }),

  selectVisible: (metas, on) =>
    set((st) => {
      const next = new Set(st.selected);
      for (const m of metas) {
        if (on) next.add(m.id);
        else next.delete(m.id);
      }
      return { selected: next };
    }),

  clearSelection: () => set({ selected: new Set<string>() }),
}));

/** Structural filter changes reset the selection (the selected rows may
 * no longer be visible); value tweaks (search text typing) keep it. */
function patchResetSelection(patch: Partial<ResultsFilter>): boolean {
  return (
    "special" in patch ||
    "categories" in patch ||
    "sizeRanges" in patch ||
    "dateRange" in patch ||
    "deletedOnly" in patch ||
    "hideOverwritten" in patch
  );
}

/* ═══════════════════════════════════════════════════════════════════════
 * Derived selectors — pure functions over the store snapshot. The views
 * call them inside useMemo keyed on (revision, filter, sort, location…).
 * ═══════════════════════════════════════════════════════════════════════ */

export function matchesFilter(m: RowMeta, f: ResultsFilter): boolean {
  if (f.special && m.group !== f.special) return false;
  if (f.categories.size && !f.categories.has(m.cat)) return false;
  if (f.deletedOnly && !m.file.was_deleted) return false;
  if (f.hideOverwritten && m.file.status === "Overwritten") return false;
  if (f.hideJunk && isJunkFile(m.file)) return false;
  if (f.hideSystem && isSystemPath(m.file.parent_path)) return false;

  if (f.sizeRanges.size) {
    const sz = m.file.size_bytes;
    let ok = false;
    for (const id of f.sizeRanges) {
      const r = SIZE_RANGES.find((x) => x.id === id);
      if (r && sz >= r.min && (r.max == null || sz < r.max)) {
        ok = true;
        break;
      }
    }
    if (!ok) return false;
  }

  if (f.dateRange === "custom") {
    const t = m.mtime?.getTime();
    if (t == null) return false;
    if (f.dateFrom != null && t < f.dateFrom) return false;
    if (f.dateTo != null && t > f.dateTo) return false;
  } else if (f.dateRange) {
    const range = DATE_RANGES.find((x) => x.id === f.dateRange);
    if (range) {
      const t = m.mtime?.getTime();
      if (t == null || t < range.from()) return false;
    }
  }

  const extQ = f.extensions.trim();
  if (extQ) {
    const wanted = extQ
      .split(/[;,]/)
      .map((s) => s.trim().replace(/^\*\./, "").replace(/^\./, "").toLowerCase())
      .filter(Boolean);
    if (wanted.length && !wanted.includes(m.file.extension.toLowerCase())) return false;
  }

  const q = f.search.trim().toLowerCase();
  if (q) {
    const name = m.file.name.toLowerCase();
    const path = m.file.parent_path.toLowerCase();
    if (!name.includes(q) && !path.includes(q)) return false;
  }

  return true;
}

export function sortMetas(metas: RowMeta[], key: SortKey, asc: boolean): RowMeta[] {
  const dir = asc ? 1 : -1;
  const out = [...metas];
  out.sort((a, b) => {
    let c = 0;
    switch (key) {
      case "name":
        c = a.file.name.localeCompare(b.file.name, undefined, { numeric: true, sensitivity: "base" });
        break;
      case "size":
        c = a.file.size_bytes - b.file.size_bytes;
        break;
      case "modified": {
        const ta = a.mtime?.getTime() ?? 0;
        const tb = b.mtime?.getTime() ?? 0;
        c = ta - tb;
        break;
      }
    }
    if (c === 0) c = a.file.name.localeCompare(b.file.name, undefined, { numeric: true });
    return c * dir;
  });
  return out;
}

/** Build the flat row model for the CURRENT view. */
export function buildRows(
  st: Pick<
    ResultsStore,
    "byId" | "childFiles" | "childDirs" | "subtree" | "filter" | "sort" | "group" | "location" | "revision" | "all"
  >,
): Row[] {
  const { filter, sort, group, location } = st;
  const loc = location;

  if (loc.view === "path") {
    /* Path view: folder rows + file rows of the current directory. */
    const dirKey = keyOf(loc.path);
    const rows: Row[] = [];

    const dirs = [...(st.childDirs.get(dirKey) ?? [])].sort((a, b) =>
      a.localeCompare(b, undefined, { numeric: true }),
    );
    for (const d of dirs) {
      const sub = st.subtree.get(d) ?? { files: 0, bytes: 0 };
      rows.push({
        kind: "dir",
        header: {
          key: d,
          label: d.slice(d.lastIndexOf("/") + 1),
          count: sub.files,
          bytes: sub.bytes,
          kind: "dir",
          path: d,
        },
      });
    }

    const metas = (st.childFiles.get(dirKey) ?? [])
      .map((id) => st.byId.get(id))
      .filter((m): m is RowMeta => m != null && matchesFilter(m, filter));
    const sorted = sortMetas(metas, sort.key, sort.asc);
    for (const m of sorted) rows.push({ kind: "file", meta: m });
    return rows;
  }

  /* Type view / grid: flat list, optionally grouped. */
  const metas: RowMeta[] = [];
  if (filter.special) {
    // The special-group tree selection: scan all, keep the group.
    for (const m of st.byId.values()) {
      if (m.group === filter.special && matchesFilter(m, filter)) metas.push(m);
    }
  } else {
    const cat = loc.category;
    for (const m of st.byId.values()) {
      if (cat && m.cat !== cat) continue;
      if (matchesFilter(m, filter)) metas.push(m);
    }
  }

  const rows: Row[] = [];

  if (group === "type" && !loc.category && !filter.special) {
    // Group headers per category (EaseUS Type view).
    const byCat = new Map<CategoryId, RowMeta[]>();
    for (const m of metas) {
      let arr = byCat.get(m.cat);
      if (!arr) {
        arr = [];
        byCat.set(m.cat, arr);
      }
      arr.push(m);
    }
    const order: CategoryId[] = [
      "photos",
      "videos",
      "audio",
      "documents",
      "archives",
      "email",
      "code",
      "other",
    ];
    for (const c of order) {
      const arr = byCat.get(c);
      if (!arr || arr.length === 0) continue;
      const bytes = arr.reduce((s, m) => s + m.file.size_bytes, 0);
      rows.push({
        kind: "group",
        header: {
          key: c,
          label: CATEGORY_LABELS[c],
          count: arr.length,
          bytes,
          cat: c,
          kind: "type",
        },
      });
      for (const m of sortMetas(arr, sort.key, sort.asc)) rows.push({ kind: "file", meta: m });
    }
    return rows;
  }

  if (group === "date" && !filter.special) {
    const byDate = new Map<number, RowMeta[]>();
    for (const m of metas) {
      const b = dateBucketOf(m.mtime);
      let arr = byDate.get(b.rank);
      if (!arr) {
        arr = [];
        byDate.set(b.rank, arr);
      }
      arr.push(m);
    }
    const ranks = [...byDate.keys()].sort((a, b) => a - b);
    for (const r of ranks) {
      const arr = byDate.get(r)!;
      const b = dateBucketOf(arr[0].mtime);
      rows.push({
        kind: "group",
        header: {
          key: b.id,
          label: b.label,
          count: arr.length,
          bytes: arr.reduce((s, m) => s + m.file.size_bytes, 0),
          kind: "date",
        },
      });
      for (const m of sortMetas(arr, sort.key, sort.asc)) rows.push({ kind: "file", meta: m });
    }
    return rows;
  }

  for (const m of sortMetas(metas, sort.key, sort.asc)) rows.push({ kind: "file", meta: m });
  return rows;
}

const CATEGORY_LABELS: Record<CategoryId, string> = {
  photos: "Photos",
  videos: "Videos",
  audio: "Audio",
  documents: "Documents",
  archives: "Archives",
  email: "Email",
  code: "Code",
  other: "Other",
};

/** Selection summary: count + bytes (the "Selected: %1 files %2" bar). */
export function selectionSummary(
  selected: Set<string>,
  byId: Map<string, RowMeta>,
): { count: number; bytes: number; files: RowMeta[] } {
  let bytes = 0;
  const files: RowMeta[] = [];
  for (const id of selected) {
    const m = byId.get(id);
    if (m) {
      bytes += m.file.size_bytes;
      files.push(m);
    }
  }
  return { count: files.length, bytes, files };
}
