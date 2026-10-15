// Overview stat cards + category breakdown row.

import type { ScanStatsDto } from "../bindings";
import { formatBytes, categoryColor } from "../lib/format";

interface Props {
  stats: ScanStatsDto | null;
  activeCategory: string | null;
  onCategory: (c: string | null) => void;
}

export function StatsOverview({ stats, activeCategory, onCategory }: Props) {
  if (!stats) {
    return (
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <div
            key={i}
            className="h-24 animate-pulse rounded-xl border border-zinc-800 bg-zinc-900/60"
          />
        ))}
      </div>
    );
  }

  const totalBytesLabel = formatBytes(stats.total_bytes);
  const best = [...stats.by_status].sort((a, b) => b.bytes - a.bytes)[0];

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <div className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 transition-colors hover:border-zinc-700">
          <p className="text-xs font-medium tracking-wide text-zinc-500 uppercase">
            Files found
          </p>
          <p className="mt-1.5 text-2xl font-semibold tabular-nums text-zinc-50">
            {stats.total_files.toLocaleString()}
          </p>
        </div>
        <div className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 transition-colors hover:border-zinc-700">
          <p className="text-xs font-medium tracking-wide text-zinc-500 uppercase">
            Total size
          </p>
          <p className="mt-1.5 text-2xl font-semibold tabular-nums text-zinc-50">
            {totalBytesLabel}
          </p>
        </div>
        <div className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 transition-colors hover:border-zinc-700">
          <p className="text-xs font-medium tracking-wide text-zinc-500 uppercase">
            Largest group
          </p>
          <p className="mt-1.5 truncate text-2xl font-semibold text-zinc-50">
            {best ? best.status.replace(/_/g, " ") : "none"}
          </p>
          {best && (
            <p className="text-xs text-zinc-500">
              {best.files.toLocaleString()} files, {formatBytes(best.bytes)}
            </p>
          )}
        </div>
      </div>

      {stats.by_category.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={() => onCategory(null)}
            className={`rounded-full border px-3 py-1 text-xs font-medium transition-all duration-200 ${
              activeCategory === null
                ? "border-blue-500/50 bg-blue-500/15 text-blue-300"
                : "border-zinc-700 bg-zinc-900 text-zinc-400 hover:border-zinc-600 hover:text-zinc-200"
            }`}
          >
            All {stats.total_files.toLocaleString()}
          </button>
          {stats.by_category
            .sort((a, b) => b.bytes - a.bytes)
            .map((c, i) => (
              <button
                key={c.category}
                onClick={() =>
                  onCategory(activeCategory === c.category ? null : c.category)
                }
                className={`flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-all duration-200 ${
                  activeCategory === c.category
                    ? "border-blue-500/50 bg-blue-500/15 text-blue-300"
                    : "border-zinc-700 bg-zinc-900 text-zinc-400 hover:border-zinc-600 hover:text-zinc-200"
                }`}
              >
                <span
                  className="h-1.5 w-1.5 rounded-full"
                  style={{ background: categoryColor(i) }}
                />
                {c.label} {c.files.toLocaleString()}
              </button>
            ))}
        </div>
      )}
    </div>
  );
}
