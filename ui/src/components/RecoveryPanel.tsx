// Recovery flow: destination selection, collision policy, live progress,
// and a per-run summary. Opened from the results toolbar when files are
// selected.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  CollisionPolicy,
  RecoverFilesResponse,
  RecoveryProgress,
} from "../bindings";
import { formatBytes } from "../lib/format";

interface Props {
  scanId: string;
  entryIds: string[];
  totalBytes: number;
  onDone: () => void;
  onClose: () => void;
}

type Phase = "setup" | "running" | "done" | "error";

export function RecoveryPanel({
  scanId,
  entryIds,
  totalBytes,
  onDone,
  onClose,
}: Props) {
  const [dest, setDest] = useState<string | null>(null);
  const [policy, setPolicy] = useState<CollisionPolicy>("keep_both");
  const [phase, setPhase] = useState<Phase>("setup");
  const [progress, setProgress] = useState<RecoveryProgress | null>(null);
  const [summary, setSummary] = useState<RecoverFilesResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    panelRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, []);

  useEffect(() => {
    if (phase !== "running") return;
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    listen<RecoveryProgress>("recovery:progress", (event) => {
      if (cancelled) return;
      if (event.payload.scan_id !== scanId) return;
      setProgress(event.payload);
    }).then((un) => {
      if (cancelled) un();
      else unlisten = un;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [phase, scanId]);

  async function pickDestination() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setDest(selected);
    }
  }

  async function startRecovery() {
    if (!dest) return;
    setPhase("running");
    setError(null);
    setProgress(null);
    try {
      const res = await invoke<RecoverFilesResponse>("recover_files", {
        req: {
          scan_id: scanId,
          entry_ids: entryIds,
          destination_dir: dest,
          collision_policy: policy,
        },
      });
      setSummary(res);
      setPhase("done");
      if (res.recovered > 0) onDone();
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  const pct =
    progress && progress.total > 0
      ? Math.round((progress.done / progress.total) * 100)
      : 0;

  return (
    <div
      ref={panelRef}
      className="animate-slide-up rounded-xl border border-blue-500/30 bg-zinc-900 p-5 shadow-lg shadow-blue-950/20"
    >
      {phase === "setup" && (
        <>
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-zinc-100">
              Recover {entryIds.length.toLocaleString()} file
              {entryIds.length === 1 ? "" : "s"}
              <span className="ml-2 font-normal text-zinc-400">
                {formatBytes(totalBytes)}
              </span>
            </h3>
            <button
              onClick={onClose}
              className="rounded p-1 text-zinc-500 transition-colors hover:bg-zinc-800 hover:text-zinc-200"
              aria-label="Close recovery panel"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <path
                  d="M2 2l10 10M12 2L2 12"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                />
              </svg>
            </button>
          </div>

          <div className="mt-4 space-y-4">
            <div>
              <label className="text-xs font-medium text-zinc-400">
                Save to
              </label>
              <button
                onClick={pickDestination}
                className="mt-1.5 flex w-full items-center justify-between rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2.5 text-left text-sm transition-colors hover:border-zinc-600"
              >
                <span className={dest ? "text-zinc-200" : "text-zinc-500"}>
                  {dest ?? "Choose a folder"}
                </span>
                <span className="text-xs font-medium text-blue-400">
                  Browse
                </span>
              </button>
              <p className="mt-1.5 text-xs text-zinc-600">
                Pick a different drive if you can. Saving to the same disk
                you are recovering from can overwrite the very files you
                want back.
              </p>
            </div>

            <div>
              <label className="text-xs font-medium text-zinc-400">
                When a file with the same name already exists
              </label>
              <div className="mt-1.5 grid grid-cols-1 gap-2 sm:grid-cols-3">
                {(
                  [
                    ["keep_both", "Keep both", "New file gets a number"],
                    ["error", "Skip it", "Leave the existing file alone"],
                    ["overwrite", "Replace", "Existing file is deleted"],
                  ] as const
                ).map(([value, label, hint]) => (
                  <button
                    key={value}
                    onClick={() => setPolicy(value)}
                    className={`rounded-lg border p-2.5 text-left transition-all duration-200 ${
                      policy === value
                        ? "border-blue-500/60 bg-blue-500/10"
                        : "border-zinc-700 bg-zinc-950 hover:border-zinc-600"
                    }`}
                  >
                    <p className="text-xs font-semibold text-zinc-100">
                      {label}
                    </p>
                    <p className="mt-0.5 text-[11px] text-zinc-500">{hint}</p>
                  </button>
                ))}
              </div>
            </div>

            <button
              onClick={startRecovery}
              disabled={!dest}
              className="w-full rounded-lg bg-blue-600 py-2.5 text-sm font-semibold text-white transition-all duration-200 hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-40"
            >
              Start recovery
            </button>
          </div>
        </>
      )}

      {phase === "running" && (
        <div>
          <h3 className="text-sm font-semibold text-zinc-100">
            Recovering files
          </h3>
          <div className="mt-4 h-2 w-full overflow-hidden rounded-full bg-zinc-800">
            <div
              className="h-full rounded-full bg-gradient-to-r from-blue-500 to-cyan-400 transition-all duration-300"
              style={{ width: `${Math.max(pct, 3)}%` }}
            />
          </div>
          <div className="mt-2 flex justify-between text-xs text-zinc-400">
            <span className="truncate pr-4">
              {progress?.current_file ?? "Preparing"}
            </span>
            <span className="tabular-nums whitespace-nowrap">
              {progress ? `${progress.done} / ${progress.total}` : ""}
              {progress && progress.bytes_written > 0 && (
                <span className="text-zinc-500">
                  {" "}
                  ({formatBytes(progress.bytes_written)})
                </span>
              )}
            </span>
          </div>
        </div>
      )}

      {phase === "done" && summary && (
        <div>
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-zinc-100">
              {summary.failed > 0
                ? `Recovered ${summary.recovered} of ${summary.recovered + summary.failed} files`
                : `Recovered ${summary.recovered} file${summary.recovered === 1 ? "" : "s"}`}
            </h3>
            <button
              onClick={onClose}
              className="rounded p-1 text-zinc-500 transition-colors hover:bg-zinc-800 hover:text-zinc-200"
              aria-label="Close summary"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <path
                  d="M2 2l10 10M12 2L2 12"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                />
              </svg>
            </button>
          </div>
          <p className="mt-1 text-xs text-zinc-400">
            {formatBytes(summary.bytes_written)} written to {dest}
          </p>

          {summary.warnings.length > 0 && (
            <div className="mt-3 space-y-1">
              {summary.warnings.slice(0, 4).map((w, i) => (
                <p
                  key={i}
                  className="rounded border border-amber-500/30 bg-amber-500/10 px-2.5 py-1.5 text-xs text-amber-300"
                >
                  {w}
                </p>
              ))}
              {summary.warnings.length > 4 && (
                <p className="text-xs text-amber-400/70">
                  and {summary.warnings.length - 4} more notes in the log
                </p>
              )}
            </div>
          )}

          {summary.failed > 0 && (
            <details className="mt-3 rounded-lg border border-zinc-800 bg-zinc-950 p-2.5">
              <summary className="cursor-pointer text-xs font-medium text-zinc-300">
                {summary.failed} file{summary.failed === 1 ? "" : "s"} failed
              </summary>
              <ul className="mt-2 space-y-1">
                {summary.errors.slice(0, 10).map((e, i) => (
                  <li key={i} className="text-xs text-zinc-500">
                    {e}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}

      {phase === "error" && (
        <div>
          <h3 className="text-sm font-semibold text-red-400">
            Recovery could not start
          </h3>
          <p className="mt-2 break-words rounded border border-red-500/30 bg-red-500/10 p-2.5 text-xs text-red-300">
            {error}
          </p>
          <button
            onClick={() => setPhase("setup")}
            className="mt-3 rounded-lg border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-800"
          >
            Back
          </button>
        </div>
      )}
    </div>
  );
}
