// Live scan progress with phase timeline and entries counter.

import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ScanProgress } from "../bindings";

interface Props {
  scanId: string;
  deep: boolean;
  onViewResults: () => void;
  onCancel: () => void;
}

const PHASES = [
  { key: "Probing", label: "Identifying the filesystem" },
  { key: "QuickScan", label: "Reading file records" },
  { key: "DeepScan", label: "Scanning unallocated space" },
  { key: "Carving", label: "Rebuilding lost files" },
  { key: "Finalizing", label: "Finishing up" },
] as const;

export function ScanningView({
  scanId,
  deep,
  onViewResults,
  onCancel,
}: Props) {
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [rate, setRate] = useState<number | null>(null);
  const firstCount = useRef<{ count: number; at: number } | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<ScanProgress>("scan:progress", (event) => {
      if (cancelled || event.payload.scan_id !== scanId) return;
      setProgress(event.payload);

      // Entries per second, measured client-side from the counter.
      const now = Date.now();
      const count = event.payload.current;
      if (firstCount.current === null && count > 0) {
        firstCount.current = { count, at: now };
      } else if (firstCount.current && count > firstCount.current.count) {
        const dt = (now - firstCount.current.at) / 1000;
        if (dt >= 1) {
          setRate((count - firstCount.current.count) / dt);
        }
      }

      if (event.payload.phase === "Done") {
        setTimeout(onViewResults, 600);
      }
    }).then((un) => {
      if (cancelled) un();
      else unlisten = un;
    });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [scanId, onViewResults]);

  const done = progress?.phase === "Done";
  const failed = progress?.phase === "Failed";
  const cancelledPhase = progress?.phase === "Cancelled";

  const activeIdx = PHASES.findIndex(
    (p) =>
      p.key === progress?.phase ||
      (progress?.phase === "DeepScan" && p.key === "QuickScan") ||
      (done && p.key === "Finalizing")
  );
  const currentLabel =
    PHASES.find((p) => p.key === progress?.phase)?.label ?? "Preparing";

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/60 p-8">
        <div className="flex flex-col items-center text-center">
          <div className="relative mb-6 h-16 w-16">
            <div
              className={`absolute inset-0 rounded-2xl bg-blue-500/20 ${
                done || failed || cancelledPhase ? "" : "animate-pulse-ring"
              }`}
            />
            <div className="absolute inset-2 flex items-center justify-center rounded-xl bg-gradient-to-br from-blue-500 to-cyan-400 text-2xl">
              {done ? (
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none">
                  <path
                    d="M5 13l4 4L19 7"
                    stroke="white"
                    strokeWidth="2.4"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              ) : failed || cancelledPhase ? (
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none">
                  <path
                    d="M6 6l12 12M18 6L6 18"
                    stroke="white"
                    strokeWidth="2.4"
                    strokeLinecap="round"
                  />
                </svg>
              ) : (
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none">
                  <path
                    d="M12 3v4m0 4v10M4.9 7.5l2.9 1.7m8.4 4.9l2.9 1.7m0-8.3l-2.9 1.7M7.8 14.1l-2.9 1.7"
                    stroke="white"
                    strokeWidth="1.9"
                    strokeLinecap="round"
                  />
                </svg>
              )}
            </div>
          </div>

          <h2 className="text-lg font-semibold">
            {done
              ? "Scan complete"
              : failed
                ? "Scan failed"
                : cancelledPhase
                  ? "Scan cancelled"
                  : currentLabel}
          </h2>
          <p className="mt-1 text-sm text-zinc-400">
            {done
              ? "Your files are ready to review."
              : failed || cancelledPhase
                ? (progress?.message ?? "")
                : progress?.message ?? "This can take a while on large disks."}
          </p>

          {(progress?.current ?? 0) > 0 && !done && (
            <p className="mt-3 text-2xl font-semibold tabular-nums text-zinc-100">
              {progress!.current.toLocaleString()}
              <span className="ml-2 text-sm font-normal text-zinc-500">
                files found{rate !== null && ` (${Math.round(rate)}/s)`}
              </span>
            </p>
          )}
        </div>

        <div className="mt-8 space-y-2.5">
          {PHASES.map((p, i) => {
            const active = i === activeIdx && !done;
            const complete = done || i < activeIdx;
            const skipped =
              !deep &&
              (p.key === "DeepScan" || p.key === "Carving");
            return (
              <div key={p.key} className="flex items-center gap-3">
                <div
                  className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-full border transition-all duration-300 ${
                    complete
                      ? "border-emerald-500 bg-emerald-500/20"
                      : active
                        ? "border-blue-400 bg-blue-500/20"
                        : skipped
                          ? "border-zinc-800 bg-zinc-900"
                          : "border-zinc-700"
                  }`}
                >
                  {complete && (
                    <svg width="10" height="10" viewBox="0 0 12 12" fill="none">
                      <path
                        d="M2.5 6.5l2.5 2.5L10 4"
                        stroke="#34d399"
                        strokeWidth="1.8"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  )}
                  {active && (
                    <div className="h-1.5 w-1.5 animate-pulse rounded-full bg-blue-400" />
                  )}
                </div>
                <span
                  className={`text-sm transition-colors duration-300 ${
                    complete
                      ? "text-zinc-400"
                      : active
                        ? "font-medium text-zinc-100"
                        : skipped
                          ? "text-zinc-700 line-through"
                          : "text-zinc-600"
                  }`}
                >
                  {p.label}
                </span>
              </div>
            );
          })}
        </div>

        <div className="mt-8 flex justify-center gap-2">
          {done ? (
            <button
              onClick={onViewResults}
              className="rounded-lg bg-blue-600 px-5 py-2 text-sm font-semibold text-white transition-all duration-200 hover:bg-blue-500"
            >
              Review files
            </button>
          ) : (
            <button
              onClick={onCancel}
              disabled={failed || cancelledPhase}
              className="rounded-lg border border-zinc-700 px-5 py-2 text-sm font-medium text-zinc-300 transition-all duration-200 hover:border-zinc-600 hover:bg-zinc-800 disabled:opacity-40"
            >
              Cancel scan
            </button>
          )}
        </div>
      </section>
    </div>
  );
}
