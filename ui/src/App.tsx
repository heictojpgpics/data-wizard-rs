import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DiskInfo, StartScanResponse } from "./bindings";

type View = "select" | "scanning" | "results";

export default function App() {
  const [view, setView] = useState<View>("select");
  const [disks, setDisks] = useState<DiskInfo[]>([]);
  const [scanId, setScanId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadingDisks, setLoadingDisks] = useState(false);

  useEffect(() => {
    refreshDisks();
  }, []);

  async function refreshDisks() {
    setLoadingDisks(true);
    setError(null);
    try {
      const res = await invoke<{ disks: DiskInfo[] }>("list_disks", {
        req: {},
      });
      setDisks(res.disks);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingDisks(false);
    }
  }

  async function startScan(sourceUri: string, deep: boolean) {
    setError(null);
    try {
      const res = await invoke<StartScanResponse>("start_scan", {
        req: {
          source_uri: sourceUri,
          deep,
          max_results: 1_000_000,
          file_filter: [],
        },
      });
      setScanId(res.scan_id);
      setView("scanning");
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-zinc-800 bg-zinc-900 px-6 py-3">
        <div className="flex items-center gap-3">
          <div className="h-8 w-8 rounded bg-blue-500" aria-hidden />
          <div>
            <h1 className="text-base font-semibold">Data Wizard</h1>
            <p className="text-xs text-zinc-400">
              Free, open-source data recovery
            </p>
          </div>
        </div>
        <div className="flex gap-2 text-xs text-zinc-400">
          <span>v0.1.0</span>
          <span aria-hidden>·</span>
          <a
            className="text-blue-400 hover:underline"
            href="https://github.com/heictojpgpics/data-wizard-rs"
            target="_blank"
            rel="noreferrer"
          >
            GitHub
          </a>
        </div>
      </header>

      <main className="flex-1 overflow-auto p-6">
        {error && (
          <div className="mb-4 rounded border border-red-700 bg-red-950/50 p-3 text-sm text-red-200">
            {error}
          </div>
        )}

        {view === "select" && (
          <SelectSource
            disks={disks}
            loading={loadingDisks}
            onRefresh={refreshDisks}
            onScan={startScan}
          />
        )}

        {view === "scanning" && scanId && (
          <ScanningView
            scanId={scanId}
            onViewResults={() => setView("results")}
            onCancel={() => {
              setScanId(null);
              setView("select");
            }}
          />
        )}

        {view === "results" && scanId && (
          <ResultsView scanId={scanId} onBack={() => setView("select")} />
        )}
      </main>
    </div>
  );
}

function SelectSource({
  disks,
  loading,
  onRefresh,
  onScan,
}: {
  disks: DiskInfo[];
  loading: boolean;
  onRefresh: () => void;
  onScan: (sourceUri: string, deep: boolean) => void;
}) {
  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <section>
        <h2 className="mb-2 text-lg font-semibold">Select a source</h2>
        <p className="text-sm text-zinc-400">
          Pick a disk or partition to scan for recoverable files. The scan is
          read-only — your data will not be modified.
        </p>
      </section>

      <section>
        <div className="mb-2 flex items-center justify-between">
          <h3 className="text-sm font-medium text-zinc-300">Available disks</h3>
          <button
            onClick={onRefresh}
            disabled={loading}
            className="rounded border border-zinc-700 px-2 py-1 text-xs hover:bg-zinc-800"
          >
            {loading ? "Refreshing..." : "Refresh"}
          </button>
        </div>
        {disks.length === 0 ? (
          <div className="rounded border border-dashed border-zinc-700 p-6 text-center text-sm text-zinc-500">
            No disks detected. This is expected in the early scaffold — disk
            enumeration is wired up next.
          </div>
        ) : (
          <ul className="divide-y divide-zinc-800 overflow-hidden rounded border border-zinc-800">
            {disks.map((d) => (
              <li
                key={d.id}
                className="flex items-center justify-between bg-zinc-900 p-3"
              >
                <div>
                  <div className="text-sm font-medium">{d.display_name}</div>
                  <div className="text-xs text-zinc-500">
                    {formatBytes(d.size_bytes)}
                    {d.is_system ? " · System" : ""}
                    {d.is_removable ? " · Removable" : ""}
                  </div>
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => onScan(d.id, false)}
                    className="rounded border border-zinc-700 px-3 py-1 text-xs hover:bg-zinc-800"
                  >
                    Quick scan
                  </button>
                  <button
                    onClick={() => onScan(d.id, true)}
                    className="rounded bg-blue-600 px-3 py-1 text-xs font-medium hover:bg-blue-700"
                  >
                    Deep scan
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function ScanningView({
  scanId,
  onViewResults,
  onCancel,
}: {
  scanId: string;
  onViewResults: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <section>
        <h2 className="mb-2 text-lg font-semibold">Scanning…</h2>
        <p className="text-sm text-zinc-400">
          Scan ID: <code className="text-zinc-300">{scanId}</code>
        </p>
      </section>
      <section className="rounded border border-zinc-800 bg-zinc-900 p-4">
        <div className="mb-3 text-sm text-zinc-400">
          The scan orchestrator is running. Progress reporting is wired up
          next — for now, this is a placeholder view.
        </div>
        <div className="flex gap-2">
          <button
            onClick={onViewResults}
            className="rounded bg-blue-600 px-3 py-1 text-sm font-medium hover:bg-blue-700"
          >
            View results
          </button>
          <button
            onClick={onCancel}
            className="rounded border border-zinc-700 px-3 py-1 text-sm hover:bg-zinc-800"
          >
            Cancel
          </button>
        </div>
      </section>
    </div>
  );
}

function ResultsView({
  scanId,
  onBack,
}: {
  scanId: string;
  onBack: () => void;
}) {
  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <section className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Recoverable files</h2>
        <button
          onClick={onBack}
          className="rounded border border-zinc-700 px-3 py-1 text-xs hover:bg-zinc-800"
        >
          ← New scan
        </button>
      </section>
      <p className="text-sm text-zinc-400">
        Scan ID: <code className="text-zinc-300">{scanId}</code>
      </p>
      <div className="rounded border border-dashed border-zinc-700 p-6 text-center text-sm text-zinc-500">
        Results list will appear here. List/pagination/recover dialogs are
        wired up next.
      </div>
    </div>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(1)} GB`;
}
