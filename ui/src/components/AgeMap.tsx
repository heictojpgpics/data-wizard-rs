// Age map: bytes attributed to each last-modified month, from the real
// scan statistics (backend aggregates FS-native mtimes; carved files
// without timestamps are reported separately, never hidden).

import { useMemo, useState } from "react";
import type { ScanStatsDto } from "../bindings";
import { formatBytes } from "../lib/format";

interface Props {
  stats: ScanStatsDto | null;
}

export function AgeMap({ stats }: Props) {
  const [hovered, setHovered] = useState<number | null>(null);

  const buckets = useMemo(() => stats?.by_month ?? [], [stats]);
  const maxBytes = useMemo(
    () => buckets.reduce((m, b) => Math.max(m, b.bytes), 0),
    [buckets]
  );

  if (!stats) {
    return (
      <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
        <h3 className="text-sm font-semibold text-zinc-200">
          Bytes by last modified month
        </h3>
        <div className="mt-8 mb-2 h-24 animate-pulse rounded-lg bg-zinc-800/50" />
        <p className="text-xs text-zinc-500">Loading statistics</p>
      </section>
    );
  }

  if (buckets.length === 0) {
    return (
      <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
        <h3 className="text-sm font-semibold text-zinc-200">
          Bytes by last modified month
        </h3>
        <div className="mt-4 rounded-lg border border-dashed border-zinc-700 p-6 text-center">
          <p className="text-sm text-zinc-400">
            No recoverable timestamps in this scan
          </p>
          <p className="mt-1 text-xs text-zinc-500">
            {stats.no_timestamp_files > 0
              ? `${stats.no_timestamp_files.toLocaleString()} files were found without usable timestamps. Carved files keep their bytes but the metadata that held the date is gone.`
              : "The source did not report modification dates for its files."}
          </p>
        </div>
      </section>
    );
  }

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
      <div className="flex items-baseline justify-between">
        <h3 className="text-sm font-semibold text-zinc-200">
          Bytes by last modified month
        </h3>
        <span className="text-xs text-zinc-500">
          {buckets.length} month{buckets.length === 1 ? "" : "s"} with data
          {stats.no_timestamp_files > 0 &&
            `, ${stats.no_timestamp_files.toLocaleString()} files without dates`}
        </span>
      </div>

      <div className="mt-4 flex items-end gap-[3px]" style={{ height: 128 }}>
        {buckets.map((b, i) => {
          const pct = maxBytes > 0 ? (b.bytes / maxBytes) * 100 : 0;
          const active = hovered === i;
          return (
            <div
              key={b.month}
              className="group relative flex h-full flex-1 flex-col justify-end"
              onMouseEnter={() => setHovered(i)}
              onMouseLeave={() => setHovered(null)}
            >
              {active && (
                <div className="pointer-events-none absolute -top-2 left-1/2 z-10 w-44 -translate-x-1/2 -translate-y-full rounded-lg border border-zinc-700 bg-zinc-950 p-2.5 text-left shadow-xl">
                  <p className="text-xs font-semibold text-zinc-100">
                    {b.month}
                  </p>
                  <p className="mt-0.5 text-xs text-zinc-300">
                    {formatBytes(b.bytes)}
                  </p>
                  <p className="text-xs text-zinc-500">
                    {b.files.toLocaleString()} file
                    {b.files === 1 ? "" : "s"}
                  </p>
                </div>
              )}
              <div
                className="w-full rounded-t transition-all duration-300"
                style={{
                  height: `${Math.max(pct, 2)}%`,
                  background: active
                    ? "linear-gradient(180deg, #67e8f9 0%, #0891b2 100%)"
                    : "linear-gradient(180deg, #22d3ee 0%, #2563eb 90%)",
                  opacity: active ? 1 : 0.85,
                }}
              />
            </div>
          );
        })}
      </div>

      <div className="mt-2 flex justify-between text-[10px] text-zinc-600">
        <span>{buckets[0].month}</span>
        <span>{buckets[buckets.length - 1].month}</span>
      </div>
    </section>
  );
}
