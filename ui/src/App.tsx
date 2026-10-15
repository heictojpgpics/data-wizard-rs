import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DiskInfo } from "./bindings";
import { ResultsView } from "./views/Results";
import { ScanningView } from "./views/Scanning";
import { SelectSource } from "./views/SelectSource";

type View = "select" | "scanning" | "results";

export default function App() {
  const [view, setView] = useState<View>("select");
  const [disks, setDisks] = useState<DiskInfo[]>([]);
  const [scanId, setScanId] = useState<string | null>(null);
  const [scanDeep, setScanDeep] = useState(false);
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
      const res = await invoke<{ scan_id: string }>("start_scan", {
        req: {
          source_uri: sourceUri,
          deep,
          max_results: 1_000_000,
          file_filter: [],
        },
      });
      setScanId(res.scan_id);
      setScanDeep(deep);
      setView("scanning");
    } catch (e) {
      setError(String(e));
    }
  }

  async function resumeScan(savedScanId: string, deep: boolean) {
    setError(null);
    try {
      const res = await invoke<{ scan_id: string }>("resume_scan", {
        req: { saved_scan_id: savedScanId, deep },
      });
      setScanId(res.scan_id);
      setScanDeep(deep);
      setView("scanning");
    } catch (e) {
      setError(String(e));
    }
  }

  async function cancelScan() {
    if (scanId) {
      try {
        await invoke("cancel_scan", { req: { scan_id: scanId } });
      } catch (e) {
        setError(String(e));
      }
    }
    setScanId(null);
    setView("select");
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 items-center justify-between border-b border-zinc-800/80 bg-zinc-900/40 px-6 py-3 backdrop-blur">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-gradient-to-br from-blue-500 to-cyan-400 shadow-lg shadow-blue-950/30">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
              <path
                d="M12 3v4m0 4v10M4.9 7.5l2.9 1.7m8.4 4.9l2.9 1.7m0-8.3l-2.9 1.7M7.8 14.1l-2.9 1.7"
                stroke="white"
                strokeWidth="1.9"
                strokeLinecap="round"
              />
            </svg>
          </div>
          <div>
            <h1 className="text-[15px] leading-tight font-semibold">
              Data Wizard
            </h1>
            <p className="text-xs leading-tight text-zinc-500">
              Free, open-source file recovery
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2 text-xs text-zinc-500">
          <span>v0.1.0</span>
          <span aria-hidden className="text-zinc-700">
            |
          </span>
          <a
            className="text-blue-400 transition-colors hover:text-blue-300"
            href="https://github.com/heictojpgpics/data-wizard-rs"
            target="_blank"
            rel="noreferrer"
          >
            GitHub
          </a>
        </div>
      </header>

      <main key={view} className="animate-view flex-1 overflow-auto p-6">
        {error && (
          <div className="mx-auto mb-4 max-w-4xl rounded-xl border border-red-700/50 bg-red-950/40 p-3.5 text-sm text-red-200">
            <div className="flex items-start justify-between gap-4">
              <p className="break-words">{error}</p>
              <button
                onClick={() => setError(null)}
                className="shrink-0 rounded p-0.5 text-red-400 transition-colors hover:text-red-200"
                aria-label="Dismiss"
              >
                <svg width="12" height="12" viewBox="0 0 14 14" fill="none">
                  <path
                    d="M2 2l10 10M12 2L2 12"
                    stroke="currentColor"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
            </div>
          </div>
        )}

        {view === "select" && (
          <SelectSource
            disks={disks}
            loading={loadingDisks}
            onRefresh={refreshDisks}
            onScan={startScan}
            onResume={resumeScan}
          />
        )}

        {view === "scanning" && scanId && (
          <ScanningView
            scanId={scanId}
            deep={scanDeep}
            onViewResults={() => setView("results")}
            onCancel={cancelScan}
          />
        )}

        {view === "results" && scanId && (
          <ResultsView scanId={scanId} onBack={() => setView("select")} />
        )}
      </main>
    </div>
  );
}
