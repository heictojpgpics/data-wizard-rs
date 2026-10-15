// Results browser: stats overview, age map, category filters, sortable
// paged table with selection, and the recovery flow.

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  ListResultsResponse,
  RecoveredFile,
  ScanStatsDto,
} from "../bindings";
import { RecoveryPanel } from "../components/RecoveryPanel";
import { StatsOverview } from "../components/StatsOverview";
import {
  formatBytes,
  formatDate,
  parseFsTime,
  statusChip,
  statusLabel,
} from "../lib/format";

const PAGE_SIZE = 100;

type SortKey = "name" | "size" | "date" | "status";

interface Props {
  scanId: string;
  onBack: () => void;
}

export function ResultsView({ scanId, onBack }: Props) {
  const [page, setPage] = useState(0);
  const [items, setItems] = useState<RecoveredFile[]>([]);
  const [total, setTotal] = useState(0);
  const [spilled, setSpilled] = useState(0);
  // `loading` is derived from which page has actually been fetched, so
  // no synchronous setState is needed inside the effect (the
  // react-hooks/set-state-in-effect rule).
  const [loadedPage, setLoadedPage] = useState(-1);
  const [stats, setStats] = useState<ScanStatsDto | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [sortKey, setSortKey] = useState<SortKey>("size");
  const [sortAsc, setSortAsc] = useState(false);
  const [query, setQuery] = useState("");
  const [activeCategory, setActiveCategory] = useState<string | null>(null);
  const [showRecovery, setShowRecovery] = useState(false);
  const [statsVersion, setStatsVersion] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const loading = loadedPage !== page;

  useEffect(() => {
    if (loadedPage === page) return;
    let cancelled = false;
    (async () => {
      try {
        const res = await invoke<ListResultsResponse>("list_results", {
          req: {
            scan_id: scanId,
            offset: page * PAGE_SIZE,
            limit: PAGE_SIZE,
            extension_filter: null,
          },
        });
        if (cancelled) return;
        setItems(res.items);
        setTotal(res.total);
        setSpilled(res.spilled);
        setLoadedPage(page);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [scanId, page, loadedPage]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await invoke<{ stats: ScanStatsDto }>("scan_stats", {
          req: { scan_id: scanId },
        });
        if (!cancelled) setStats(res.stats);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [scanId, statsVersion]);

  // Sort + filter (client-side within the loaded page; server-side
  // filtering arrives with the query API).
  const visible = useMemo(() => {
    let rows = items;
    if (query.trim()) {
      const q = query.trim().toLowerCase();
      rows = rows.filter(
        (f) =>
          f.name.toLowerCase().includes(q) ||
          f.parent_path.toLowerCase().includes(q)
      );
    }
    if (activeCategory && stats) {
      const cat = stats.by_category.find((c) => c.category === activeCategory);
      if (cat) {
        // Approximate client-side by extension sets is fragile; keep the
        // filter honest by matching the category's file count against the
        // backend's own grouping is not possible per-row here. Instead we
        // filter rows by the category's dominant extensions.
        rows = rows.filter((f) => extensionInCategory(f.extension, cat.category));
      }
    }
    const dir = sortAsc ? 1 : -1;
    return [...rows].sort((a, b) => {
      switch (sortKey) {
        case "name":
          return dir * a.name.localeCompare(b.name, undefined, { numeric: true });
        case "size":
          return dir * (a.size_bytes - b.size_bytes);
        case "date": {
          const da = parseFsTime(a.mtime)?.getTime() ?? 0;
          const db = parseFsTime(b.mtime)?.getTime() ?? 0;
          return dir * (da - db);
        }
        case "status":
          return dir * a.status.localeCompare(b.status);
      }
    });
  }, [items, query, activeCategory, stats, sortKey, sortAsc]);

  const selectedFiles = useMemo(
    () => items.filter((f) => selected.has(f.id)),
    [items, selected]
  );
  const selectedBytes = selectedFiles.reduce((s, f) => s + f.size_bytes, 0);

  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const pageStart = total > 0 ? page * PAGE_SIZE + 1 : 0;
  const pageEnd = Math.min((page + 1) * PAGE_SIZE, total);

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function togglePage() {
    setSelected((prev) => {
      const allOn = visible.every((f) => prev.has(f.id));
      const next = new Set(prev);
      for (const f of visible) {
        if (allOn) next.delete(f.id);
        else next.add(f.id);
      }
      return next;
    });
  }

  function setSort(key: SortKey) {
    if (key === sortKey) setSortAsc((a) => !a);
    else {
      setSortKey(key);
      setSortAsc(key === "name");
    }
  }

  const sortIndicator = (key: SortKey) =>
    sortKey === key ? (sortAsc ? " \u2191" : " \u2193") : "";

  return (
    <div className="mx-auto max-w-6xl space-y-5">
      <section className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Recoverable files</h2>
          <p className="text-xs text-zinc-500">
            {total.toLocaleString()} files found
            {spilled > 0 && ` (${spilled.toLocaleString()} loaded from disk)`}
          </p>
        </div>
        <button
          onClick={onBack}
          className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-300 transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800"
        >
          New scan
        </button>
      </section>

      {error && (
        <div className="rounded-lg border border-red-700/50 bg-red-950/40 p-3 text-sm text-red-200">
          {error}
        </div>
      )}

      <StatsOverview
        stats={stats}
        activeCategory={activeCategory}
        onCategory={setActiveCategory}
      />

      {showRecovery && selected.size > 0 && (
        <RecoveryPanel
          scanId={scanId}
          entryIds={[...selected]}
          totalBytes={selectedBytes}
          onDone={() => {
            // Re-aggregate: recovered files keep their entries, but the
            // refresh keeps the overview honest after a partial recovery.
            setStatsVersion((v) => v + 1);
          }}
          onClose={() => setShowRecovery(false)}
        />
      )}

      <section className="overflow-hidden rounded-xl border border-zinc-800">
        <div className="flex flex-wrap items-center gap-2 border-b border-zinc-800 bg-zinc-900/80 px-3 py-2.5">
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Filter this page"
            className="w-56 rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-200 placeholder-zinc-600 transition-colors focus:border-blue-500 focus:outline-none"
          />
          <span className="text-xs text-zinc-600">
            {visible.length} of {items.length} on this page
          </span>
          <div className="ml-auto flex items-center gap-2">
            {selected.size > 0 && (
              <button
                onClick={() => setShowRecovery(true)}
                className="rounded-lg bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white transition-all duration-200 hover:bg-blue-500"
              >
                Recover {selected.size.toLocaleString()} (
                {formatBytes(selectedBytes)})
              </button>
            )}
            {selected.size > 0 && (
              <button
                onClick={() => setSelected(new Set())}
                className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-400 transition-colors hover:bg-zinc-800"
              >
                Clear
              </button>
            )}
          </div>
        </div>

        {loading ? (
          <div className="space-y-1 p-3">
            {[0, 1, 2, 3, 4].map((i) => (
              <div
                key={i}
                className="h-9 animate-pulse rounded-lg bg-zinc-800/40"
                style={{ animationDelay: `${i * 80}ms` }}
              />
            ))}
          </div>
        ) : visible.length === 0 ? (
          <div className="p-10 text-center">
            <p className="text-sm text-zinc-400">
              {total === 0
                ? "No files were found on this source."
                : "Nothing on this page matches the current filter."}
            </p>
            {total === 0 && (
              <p className="mt-1 text-xs text-zinc-600">
                Try a deep scan, or pick a different partition.
              </p>
            )}
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-zinc-900/60 text-xs text-zinc-500">
                <tr>
                  <th className="w-8 px-3 py-2">
                    <input
                      type="checkbox"
                      checked={visible.length > 0 && visible.every((f) => selected.has(f.id))}
                      onChange={togglePage}
                      className="h-3.5 w-3.5 cursor-pointer rounded border-zinc-600 accent-blue-500"
                      aria-label="Select all on page"
                    />
                  </th>
                  {(
                    [
                      ["name", "Name"],
                      ["size", "Size"],
                      ["date", "Modified"],
                      ["status", "Status"],
                    ] as const
                  ).map(([key, label]) => (
                    <th key={key} className="px-3 py-2 text-left font-medium">
                      <button
                        onClick={() => setSort(key)}
                        className={`transition-colors hover:text-zinc-200 ${
                          sortKey === key ? "text-zinc-200" : ""
                        }`}
                      >
                        {label}
                        {sortIndicator(key)}
                      </button>
                    </th>
                  ))}
                  <th className="px-3 py-2 text-center font-medium">Source</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-800/80">
                {visible.map((f) => (
                  <tr
                    key={f.id}
                    onClick={() => toggle(f.id)}
                    className={`cursor-pointer transition-colors duration-150 ${
                      selected.has(f.id) ? "bg-blue-500/5" : "hover:bg-zinc-900/50"
                    }`}
                  >
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={selected.has(f.id)}
                        onChange={() => toggle(f.id)}
                        onClick={(e) => e.stopPropagation()}
                        className="h-3.5 w-3.5 cursor-pointer rounded border-zinc-600 accent-blue-500"
                        aria-label={`Select ${f.name}`}
                      />
                    </td>
                    <td className="max-w-56 truncate px-3 py-2 font-mono text-[13px] text-zinc-200">
                      {f.name}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-right tabular-nums text-zinc-400">
                      {formatBytes(f.size_bytes)}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-zinc-500">
                      {formatDate(parseFsTime(f.mtime))}
                    </td>
                    <td className="px-3 py-2">
                      <span
                        className={`inline-block rounded-full border px-2 py-0.5 text-[11px] font-medium ${statusChip(f.status)}`}
                      >
                        {statusLabel(f.status)}
                      </span>
                    </td>
                    <td className="px-3 py-2 text-center text-xs text-zinc-500">
                      {f.was_deleted ? (
                        <span className="text-amber-400/90">deleted</span>
                      ) : (
                        <span className="text-zinc-600">live</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {total > PAGE_SIZE && (
          <div className="flex items-center justify-between border-t border-zinc-800 bg-zinc-900/60 px-3 py-2 text-xs text-zinc-400">
            <span className="tabular-nums">
              {pageStart.toLocaleString()} to {pageEnd.toLocaleString()} of{" "}
              {total.toLocaleString()}
            </span>
            <div className="flex items-center gap-1.5">
              <button
                onClick={() => setPage((p) => Math.max(0, p - 1))}
                disabled={page === 0}
                className="rounded-lg border border-zinc-700 px-2.5 py-1 font-medium transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-30"
              >
                Previous
              </button>
              <span className="px-1 tabular-nums">
                {page + 1} / {totalPages}
              </span>
              <button
                onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
                disabled={page >= totalPages - 1}
                className="rounded-lg border border-zinc-700 px-2.5 py-1 font-medium transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-30"
              >
                Next
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

/** Client-side extension -> category mapping mirroring dw-core::classify. */
function extensionInCategory(ext: string, category: string): boolean {
  const sets: Record<string, string[]> = {
    photos: [
      "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "tif", "tiff",
      "webp", "heic", "heif", "avif", "ico", "psd", "svg", "dng", "cr2",
      "cr3", "nef", "arw", "orf", "rw2", "raf", "raw",
    ],
    videos: [
      "mp4", "m4v", "mov", "avi", "mkv", "webm", "wmv", "flv", "f4v",
      "mpg", "mpeg", "mp2", "3gp", "3g2", "mts", "m2ts", "mxf", "vob", "ogv",
    ],
    audio: [
      "mp3", "m4a", "aac", "wav", "flac", "ogg", "oga", "opus", "wma",
      "aiff", "aif", "mid", "midi", "amr",
    ],
    documents: [
      "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods",
      "odp", "rtf", "txt", "md", "csv", "tsv", "epub", "pages", "numbers",
      "key", "hwp",
    ],
    archives: [
      "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "lz4", "tgz",
      "tbz2", "txz", "cab", "iso", "dmg", "vhd", "vhdx", "wim", "img", "dd",
      "vmdk",
    ],
    email: ["pst", "ost", "eml", "msg", "mbox", "dbx"],
    code: [
      "rs", "c", "h", "cpp", "hpp", "cc", "py", "js", "jsx", "ts", "tsx",
      "java", "kt", "go", "rb", "php", "cs", "swift", "m", "mm", "sh",
      "ps1", "bat", "cmd", "html", "htm", "css", "scss", "json", "xml",
      "yml", "yaml", "toml", "ini", "cfg", "conf", "sql", "ipynb",
    ],
  };
  return (sets[category] ?? []).includes(ext);
}
