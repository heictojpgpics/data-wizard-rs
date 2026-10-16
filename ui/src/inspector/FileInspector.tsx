/**
 * File inspector — the details panel for the selected result row:
 * identity, timeline, status explanation, confidence meter and the
 * per-file recovery action. Slides in over the results view.
 */
import { useState } from "react";
import { invoke } from "../lib/ipc";
import type { FileRecoveryOutcome, RecoveredFile } from "../bindings";
import {
  CategoryGlyph,
  CheckIcon,
  DownloadIcon,
  FolderIcon,
  InfoIcon,
  XIcon,
} from "../lib/Icon";
import {
  categoryOf,
  categoryVar,
  fileKindLabel,
  formatBytes,
  formatDateTime,
  parseFsTime,
  statusExplanation,
  statusLabel,
  statusVar,
} from "../lib/format";
import { useAppStore } from "../state/app";

export function FileInspector() {
  const inspectedId = useAppStore((s) => s.inspectedId);
  const setInspectedId = useAppStore((s) => s.setInspectedId);
  const rows = useAppStore((s) => s.rows);
  const selected = useAppStore((s) => s.selected);
  const toggleSelected = useAppStore((s) => s.toggleSelected);
  const setToast = useAppStore((s) => s.setToast);

  const [busyId, setBusyId] = useState<string | null>(null);
  const file = rows.find((r) => r.id === inspectedId) ?? null;

  if (!file) return null;

  const busy = busyId === file.id;

  const cat = categoryOf(file.extension);
  const color = categoryVar(cat);
  const date = parseFsTime(file.mtime);
  const isSelected = selected.has(file.id);

  async function recoverOne(f: RecoveredFile) {
    setBusyId(f.id);
    try {
      const dest = await pickDestination();
      if (!dest) {
        setBusyId(null);
        return;
      }
      const res = await invoke<{
        recovered: number;
        failed: number;
        results: FileRecoveryOutcome[];
      }>("recover_files", {
        req: {
          scan_id: useAppStore.getState().scanId,
          entry_ids: [f.id],
          destination_dir: dest,
          collision_policy: "keep_both",
        },
      });
      const out = res.results[0];
      if (out?.ok) {
        setToast({ text: `Recovered ${out.name} to ${out.path ?? dest}`, kind: "ok" });
      } else {
        setToast({ text: `Could not recover ${f.name}: ${out?.error ?? "unknown error"}`, kind: "err" });
      }
    } catch (e) {
      setToast({ text: `Recovery failed: ${String(e)}`, kind: "err" });
    } finally {
      setBusyId(null);
    }
  }

  return (
    <aside className="dw-inspector" aria-label="File details">
      <div className="dw-inspector-head">
        <span className="dw-inspector-title">File details</span>
        <button
          className="dw-btn-ghost dw-icon"
          aria-label="Close details"
          onClick={() => setInspectedId(null)}
        >
          <XIcon size={14} />
        </button>
      </div>

      <div className="dw-inspector-body">
        {/* Identity block */}
        <div className="dw-fi-hero">
          <span className="dw-fi-glyph" style={{ color, background: `color-mix(in srgb, ${color} 16%, transparent)` }}>
            <CategoryGlyph cat={cat} size={26} />
          </span>
          <div className="dw-fi-id">
            <span className="dw-fi-name dw-selectable-text" title={file.name}>
              {file.name}
            </span>
            <span className="dw-fi-kind">
              {fileKindLabel(file.extension)} · {file.extension.toUpperCase() || "no extension"}
            </span>
          </div>
        </div>

        <div className="dw-fi-badges">
          <span className="dw-status-pill dw-lg" style={{ color: statusVar(file.status), borderColor: statusVar(file.status) }}>
            {statusLabel(file.status)}
          </span>
          {file.was_deleted && <span className="dw-fi-deleted">Deleted on source</span>}
        </div>

        <p className="dw-fi-explain">{statusExplanation(file.status)}</p>

        {/* Facts */}
        <dl className="dw-fi-facts">
          <div className="dw-fi-fact">
            <dt>Size</dt>
            <dd>{formatBytes(file.size_bytes)}</dd>
          </div>
          <div className="dw-fi-fact">
            <dt>Modified</dt>
            <dd>{formatDateTime(date)}</dd>
          </div>
          <div className="dw-fi-fact dw-fi-path">
            <dt>Original folder</dt>
            <dd className="dw-selectable-text">
              <FolderIcon size={12} />
              {file.parent_path}
            </dd>
          </div>
        </dl>

        {/* Confidence meter */}
        <div className="dw-fi-conf">
          <div className="dw-fi-conf-head">
            <span>Recovery confidence</span>
            <span className="dw-fi-conf-num">{file.confidence}%</span>
          </div>
          <div className="dw-track">
            <div
              className="dw-track-fill"
              style={{ width: `${Math.max(3, file.confidence)}%`, background: statusVar(file.status) }}
            />
          </div>
          <p className="dw-fi-conf-note">
            How strongly the engine believes this file will come back whole —
            from intact metadata (high) to a bare header match (low).
          </p>
        </div>

        <InfoNote />
      </div>

      {/* Actions */}
      <div className="dw-inspector-foot">
        <button
          className="dw-btn-outline"
          onClick={() => toggleSelected(file.id)}
        >
          <CheckIcon size={14} />
          {isSelected ? "Unselect" : "Add to selection"}
        </button>
        <button className="dw-btn-ink" disabled={busy} onClick={() => void recoverOne(file)}>
          {busy ? <span className="dw-spinner" /> : <DownloadIcon size={14} />}
          Recover this file
        </button>
      </div>
    </aside>
  );
}

function InfoNote() {
  return (
    <div className="dw-fi-note">
      <InfoIcon size={14} />
      <span>
        Recovery copies from the source to a destination you pick — the
        source itself is never written to.
      </span>
    </div>
  );
}

/** Destination picker: the native dialog in Tauri, a stand-in path in
 * the browser mock. */
export async function pickDestination(): Promise<string | null> {
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const chosen = await open({ title: "Choose a recovery destination", directory: true, multiple: false });
    if (typeof chosen === "string" && chosen.length > 0) return chosen;
    return null;
  } catch {
    // Browser mock: simulate a chosen folder.
    return "C:\\Recovered";
  }
}
