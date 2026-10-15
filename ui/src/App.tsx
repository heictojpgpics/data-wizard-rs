import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DiskInfo,
  RecoveredFile,
  ScanProgress,
  StartScanResponse,
} from "./bindings";

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
            onCancel={async () => {
              if (scanId) {
                try {
                  await invoke("cancel_scan", { req: { scan_id: scanId } });
                } catch (e) {
                  setError(String(e));
                }
              }
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
          <h3 className="text-sm font-medium text-zinc-300">
            Available disks
          </h3>
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
            No disks detected. Disk enumeration will be wired up in a follow-up.
            For now, you can test with a file-backed image by entering its path
            below.
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

      <ManualSourceEntry onScan={onScan} />
    </div>
  );
}

function ManualSourceEntry({
  onScan,
}: {
  onScan: (sourceUri: string, deep: boolean) => void;
}) {
  const [path, setPath] = useState("");
  return (
    <section className="rounded border border-zinc-800 bg-zinc-900 p-4">
      <h3 className="mb-2 text-sm font-medium text-zinc-300">
        Scan a file-backed image
      </h3>
      <p className="mb-3 text-xs text-zinc-500">
        Enter the path to a raw disk image (.img, .dd, .raw, .vhd) or any file
        you want to scan with the NTFS/FAT parsers.
      </p>
      <div className="flex gap-2">
        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="/path/to/image.img"
          className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-3 py-1.5 text-sm font-mono text-zinc-200 placeholder-zinc-600 focus:border-blue-500 focus:outline-none"
        />
        <button
          onClick={() => onScan(path, false)}
          disabled={!path}
          className="rounded border border-zinc-700 px-3 py-1.5 text-sm hover:bg-zinc-800"
        >
          Quick scan
        </button>
        <button
          onClick={() => onScan(path, true)}
          disabled={!path}
          className="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium hover:bg-blue-700"
        >
          Deep scan
        </button>
      </div>
    </section>
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
  const [progress, setProgress] = useState<ScanProgress | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<ScanProgress>("scan:progress", (event) => {
      if (cancelled) return;
      if (event.payload.scan_id !== scanId) return;
      setProgress(event.payload);

      // Auto-navigate to results when the scan is done.
      if (
        event.payload.phase === "Done" ||
        event.payload.phase === "Failed" ||
        event.payload.phase === "Cancelled"
      ) {
        if (event.payload.phase === "Done") {
          setTimeout(onViewResults, 500);
        }
      }
    }).then((un) => {
      if (cancelled) {
        un();
      } else {
        unlisten = un;
      }
    });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [scanId, onViewResults]);

  const phaseLabel = phaseToLabel(progress?.phase);
  const pct =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.current / progress.total) * 100))
      : 0;

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <section>
        <h2 className="mb-2 text-lg font-semibold">Scanning…</h2>
        <p className="text-sm text-zinc-400">
          Scan ID: <code className="text-zinc-300">{scanId}</code>
        </p>
      </section>

      <section className="rounded border border-zinc-800 bg-zinc-900 p-4">
        <div className="mb-2 flex items-center justify-between text-sm">
          <span className="font-medium text-zinc-200">{phaseLabel}</span>
          {progress && progress.total > 0 && (
            <span className="text-zinc-400">{pct}%</span>
          )}
        </div>

        <div className="mb-3 h-3 w-full overflow-hidden rounded-full bg-zinc-800">
          <div
            className="h-full rounded-full bg-blue-500 transition-all duration-300"
            style={{ width: `${pct}%` }}
          />
        </div>

        {progress && (
          <div className="space-y-1 text-xs text-zinc-400">
            {progress.message && <p>{progress.message}</p>}
            {progress.total > 0 && (
              <p>
                {progress.current.toLocaleString()} /{" "}
                {progress.total.toLocaleString()}
              </p>
            )}
          </div>
        )}

        <div className="mt-4 flex gap-2">
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
            Cancel scan
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
  const [files, setFiles] = useState<RecoveredFile[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadResults();
  }, [scanId]);

  async function loadResults() {
    setLoading(true);
    try {
      const res = await invoke<{
        total: number;
        spilled: number;
        items: RecoveredFile[];
      }>("list_results", {
        req: {
          scan_id: scanId,
          offset: 0,
          limit: 500,
          extension_filter: null,
        },
      });
      setFiles(res.items);
      setTotal(res.total);
    } catch (e) {
      console.error("list_results error:", e);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <section className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Recoverable files</h2>
          <p className="text-xs text-zinc-500">
            {total.toLocaleString()} files found
          </p>
        </div>
        <button
          onClick={onBack}
          className="rounded border border-zinc-700 px-3 py-1 text-xs hover:bg-zinc-800"
        >
          ← New scan
        </button>
      </section>

      {loading ? (
        <div className="rounded border border-dashed border-zinc-700 p-6 text-center text-sm text-zinc-500">
          Loading results…
        </div>
      ) : files.length === 0 ? (
        <div className="rounded border border-dashed border-zinc-700 p-6 text-center text-sm text-zinc-500">
          No files recovered. The scan may not have found any recoverable
          entries, or the filesystem parser doesn't support this source.
        </div>
      ) : (
        <div className="overflow-hidden rounded border border-zinc-800">
          <table className="w-full text-sm">
            <thead className="bg-zinc-900 text-xs text-zinc-400">
              <tr>
                <th className="px-3 py-2 text-left font-medium">Name</th>
                <th className="px-3 py-2 text-right font-medium">Size</th>
                <th className="px-3 py-2 text-left font-medium">Path</th>
                <th className="px-3 py-2 text-center font-medium">Status</th>
                <th className="px-3 py-2 text-center font-medium">Deleted?</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-800">
              {files.map((f) => (
                <tr key={f.id} className="hover:bg-zinc-900/50">
                  <td className="px-3 py-2 font-mono text-zinc-200">
                    {f.name}
                  </td>
                  <td className="px-3 py-2 text-right text-zinc-400">
                    {formatBytes(f.size_bytes)}
                  </td>
                  <td className="px-3 py-2 text-zinc-500">
                    {f.parent_path}
                  </td>
                  <td className="px-3 py-2 text-center">
                    <StatusBadge status={f.status} />
                  </td>
                  <td className="px-3 py-2 text-center">
                    {f.was_deleted ? (
                      <span className="text-amber-400">Yes</span>
                    ) : (
                      <span className="text-zinc-600">No</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    Intact: "text-green-400",
    PartiallyRecovered: "text-amber-400",
    Overwritten: "text-red-400",
    Carved: "text-blue-400",
    PendingValidation: "text-zinc-400",
  };
  return (
    <span className={colors[status] || "text-zinc-400"}>{status}</span>
  );
}

function phaseToLabel(phase?: string): string {
  switch (phase) {
    case "Probing":
      return "Probing source…";
    case "QuickScan":
      return "Quick scan (metadata walk)…";
    case "DeepScan":
      return "Deep scan (metadata + carving)…";
    case "Carving":
      return "Carving unallocated regions…";
    case "Finalizing":
      return "Finalizing…";
    case "Done":
      return "Scan complete";
    case "Cancelled":
      return "Scan cancelled";
    case "Failed":
      return "Scan failed";
    default:
      return "Starting…";
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(1)} GB`;
}
