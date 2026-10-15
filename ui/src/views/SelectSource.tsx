// Source selection: physical disks and volumes (Windows), file-backed
// images everywhere, and saved scans to resume.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DiskInfo } from "../bindings";
import { formatBytes } from "../lib/format";

interface SavedScan {
  scan_id: string;
  source_uri: string;
  created_at: string;
  mode: string;
  state: string;
  entries: number;
}

interface Props {
  disks: DiskInfo[];
  loading: boolean;
  onRefresh: () => void;
  onScan: (sourceUri: string, deep: boolean) => void;
  onResume: (savedScanId: string, deep: boolean) => void;
}

export function SelectSource({
  disks,
  loading,
  onRefresh,
  onScan,
  onResume,
}: Props) {
  const [saved, setSaved] = useState<SavedScan[]>([]);

  useEffect(() => {
    invoke<{ scans: SavedScan[] }>("list_saved_scans", { req: {} })
      .then((res) => setSaved(res.scans))
      .catch(() => setSaved([]));
  }, []);

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <section>
        <h2 className="mb-1 text-lg font-semibold">
          Choose what to recover
        </h2>
        <p className="text-sm text-zinc-400">
          Pick a disk, a partition, or a disk image. Scanning is read-only:
          nothing on the source is modified.
        </p>
      </section>

      <section>
        <div className="mb-2 flex items-center justify-between">
          <h3 className="text-sm font-medium text-zinc-300">Drives</h3>
          <button
            onClick={onRefresh}
            disabled={loading}
            className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-300 transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800 disabled:opacity-50"
          >
            {loading ? "Refreshing" : "Refresh"}
          </button>
        </div>
        {disks.length === 0 ? (
          <div className="rounded-xl border border-dashed border-zinc-700 p-6 text-center text-sm text-zinc-500">
            No drives appeared. On Windows, run the app as administrator for
            full disk access, or scan an image file below.
          </div>
        ) : (
          <ul className="divide-y divide-zinc-800 overflow-hidden rounded-xl border border-zinc-800">
            {disks.map((d) => (
              <li
                key={d.id}
                className="flex items-center justify-between bg-zinc-900/60 p-3.5 transition-colors hover:bg-zinc-900"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2 text-sm font-medium">
                    {d.display_name}
                    {d.is_system && (
                      <span className="rounded-full border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-400">
                        System
                      </span>
                    )}
                    {d.is_removable && (
                      <span className="rounded-full border border-sky-500/30 bg-sky-500/10 px-2 py-0.5 text-[11px] font-medium text-sky-400">
                        Removable
                      </span>
                    )}
                  </div>
                  <div className="mt-0.5 text-xs text-zinc-500">
                    {formatBytes(d.size_bytes)}
                    {d.partitions.length > 0 &&
                      ` with ${d.partitions.length} partition${d.partitions.length === 1 ? "" : "s"}`}
                  </div>
                </div>
                <div className="flex shrink-0 gap-2">
                  <button
                    onClick={() => onScan(d.id, false)}
                    className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800"
                  >
                    Quick scan
                  </button>
                  <button
                    onClick={() => onScan(d.id, true)}
                    className="rounded-lg bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white transition-all duration-200 hover:bg-blue-500"
                  >
                    Deep scan
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      <ImageEntry onScan={onScan} />

      {saved.length > 0 && (
        <section>
          <h3 className="mb-2 text-sm font-medium text-zinc-300">
            Saved scans
          </h3>
          <ul className="divide-y divide-zinc-800 overflow-hidden rounded-xl border border-zinc-800">
            {saved.map((s) => (
              <li
                key={s.scan_id}
                className="flex items-center justify-between bg-zinc-900/60 p-3.5"
              >
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-zinc-200">
                    {s.source_uri}
                  </div>
                  <div className="mt-0.5 text-xs text-zinc-500">
                    {s.entries.toLocaleString()} entries, {s.mode} scan,
                    saved {new Date(s.created_at).toLocaleString()}
                  </div>
                </div>
                <div className="flex shrink-0 gap-2">
                  <button
                    onClick={() => onResume(s.scan_id, false)}
                    className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800"
                  >
                    Resume quick
                  </button>
                  <button
                    onClick={() => onResume(s.scan_id, true)}
                    className="rounded-lg bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white transition-all duration-200 hover:bg-blue-500"
                  >
                    Resume deep
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function ImageEntry({
  onScan,
}: {
  onScan: (sourceUri: string, deep: boolean) => void;
}) {
  const [path, setPath] = useState("");
  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4">
      <h3 className="mb-1 text-sm font-medium text-zinc-300">
        Scan an image file
      </h3>
      <p className="mb-3 text-xs text-zinc-500">
        Raw disk images (.img, .dd, .raw, .vhd) and single files carved with
        the deep parsers.
      </p>
      <div className="flex gap-2">
        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="D:\images\disk.img"
          className="flex-1 rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-200 placeholder-zinc-600 transition-colors focus:border-blue-500 focus:outline-none"
        />
        <button
          onClick={() => onScan(path, false)}
          disabled={!path}
          className="rounded-lg border border-zinc-700 px-3 py-2 text-sm font-medium text-zinc-200 transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-40"
        >
          Quick
        </button>
        <button
          onClick={() => onScan(path, true)}
          disabled={!path}
          className="rounded-lg bg-blue-600 px-3 py-2 text-sm font-semibold text-white transition-all duration-200 hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-40"
        >
          Deep
        </button>
      </div>
    </section>
  );
}
