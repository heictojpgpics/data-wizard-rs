/**
 * FileInspector — the right-pane detail view (EaseUS PreviewMainDialog
 * role, docked). Shows: type-tinted preview tile (per-category), the
 * full path, size/dates/status/recoverability, and the recover action.
 * Doubles as the closest thing to a preview until binary previewers
 * land (the tile + metadata is honest about that).
 */

import { DownloadIcon, XIcon, MapPinIcon, CalendarIcon, LayersIcon } from "../lib/Icon";
import { formatBytes, formatDateTime, statusLabel, statusExplanation, statusVar } from "../lib/format";
import { useResultsStore } from "../state/results";
import { useRecoveryStore } from "../state/recovery";
import { TypeIcon } from "../components/ui/TypeIcon";
import { RecoverabilityBadge } from "../components/ui/RecoverabilityBadge";

export function FileInspector({ id, onClose }: { id: string; onClose: () => void }) {
  const meta = useResultsStore((s) => s.byId.get(id));
  const selected = useResultsStore((s) => s.selected);
  const toggleSelected = useResultsStore((s) => s.toggleSelected);
  const openRecovery = useRecoveryStore((s) => s.open);

  if (!meta) return null;
  const m = meta;
  const f = m.file;
  const isSel = selected.has(id);

  return (
    <aside className="dw-inspector dw-slide-up" aria-label="File details">
      <div className="dw-insp-head">
        <span>File details</span>
        <button className="dw-icon-btn dw-sm" aria-label="Close details" onClick={onClose}>
          <XIcon size={14} />
        </button>
      </div>

      <div className="dw-insp-scroll">
        {/* Preview tile */}
        <div className="dw-insp-preview" data-cat={m.cat}>
          <TypeIcon name={f.name} extension={f.extension} size={64} badge={false} />
          <span className="dw-insp-ext">.{f.extension || "—"}</span>
        </div>

        <h3 className="dw-insp-name dw-selectable-text" title={f.name}>
          {f.name}
        </h3>
        <RecoverabilityBadge status={f.status} confidence={f.confidence} />

        {/* Metadata table */}
        <dl className="dw-insp-meta">
          <div className="dw-insp-row">
            <dt>
              <MapPinIcon size={12} /> Location
            </dt>
            <dd className="dw-selectable-text" title={`${f.parent_path}/${f.name}`}>
              {f.parent_path}
            </dd>
          </div>
          <div className="dw-insp-row">
            <dt>
              <LayersIcon size={12} /> Size
            </dt>
            <dd className="tnum">{formatBytes(f.size_bytes)}</dd>
          </div>
          <div className="dw-insp-row">
            <dt>
              <CalendarIcon size={12} /> Modified
            </dt>
            <dd>{m.mtime ? formatDateTime(m.mtime) : "unknown"}</dd>
          </div>
          <div className="dw-insp-row">
            <dt>Status</dt>
            <dd style={{ color: statusVar(f.status) }}>{statusLabel(f.status)}</dd>
          </div>
          <div className="dw-insp-row">
            <dt>Origin</dt>
            <dd>
              {m.group === "deleted"
                ? "Deleted from the filesystem"
                : m.group === "recycled"
                  ? "From the Recycle Bin"
                  : m.group === "lost"
                    ? "Carved from unallocated space"
                    : "Still present on the volume"}
            </dd>
          </div>
          <div className="dw-insp-row">
            <dt>MIME</dt>
            <dd>{f.mime ?? "unknown"}</dd>
          </div>
        </dl>

        <p className="dw-insp-explain">{statusExplanation(f.status)}</p>
      </div>

      <div className="dw-insp-actions">
        <button
          className={`dw-btn-outline${isSel ? " dw-on" : ""}`}
          onClick={() => toggleSelected(id)}
        >
          {isSel ? "Deselect" : "Select for recovery"}
        </button>
        <button
          className="dw-btn-ink"
          onClick={() => openRecovery([m])}
          disabled={f.status === "Overwritten"}
        >
          <DownloadIcon size={15} />
          Recover this file
        </button>
      </div>
    </aside>
  );
}
