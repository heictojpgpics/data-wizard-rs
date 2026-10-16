/**
 * Source selection (the landing view): hero + trust strip, drive cards
 * with expandable partition rows (partition-scoped scans), the disk
 * image opener, and saved scans to resume.
 */
import { useRef, useState } from "react";
import { invoke } from "../lib/ipc";
import type { DiskInfo, PartitionInfo, SavedScanSummary } from "../bindings";
import {
  ChevronRightIcon,
  ClockIcon,
  DiscIcon,
  HardDriveIcon,
  HistoryIcon,
  PlayIcon,
  RefreshIcon,
  ScanSearchIcon,
  UsbIcon,
  WandIcon,
  DatabaseIcon,
} from "../lib/Icon";
import { busLabel, formatBytes, timeAgo } from "../lib/format";
import { useAppStore } from "../state/app";

interface Props {
  onScan: (source: { uri: string; label: string; detail: string; partition: number | null }, deep: boolean) => void;
  onResume: (saved: SavedScanSummary) => void;
}

const TRUST = [
  "Scanning is strictly read-only",
  "Nothing on the source is modified",
  "Files recover to a drive you choose",
];

function DiskGlyph({ disk }: { disk: DiskInfo }) {
  const Icon = disk.is_removable ? UsbIcon : disk.bus_type === "Nvme" ? DiscIcon : HardDriveIcon;
  return <Icon size={20} />;
}

function PartitionRow({
  part,
  disk,
  onScan,
}: {
  part: PartitionInfo;
  disk: DiskInfo;
  onScan: Props["onScan"];
}) {
  const fs = part.fs_kind?.toUpperCase() ?? "UNKNOWN";
  const label = part.drive_letter
    ? `${part.drive_letter} ${part.label ?? ""}`.trim()
    : part.label ?? `Partition ${part.index}`;
  return (
    <div className="dw-part-row">
      <div className="dw-part-id">
        <span className="dw-part-name dw-selectable-text">{label}</span>
        <span className="dw-part-fs">{fs}</span>
        {part.is_system && <span className="dw-part-badge">System</span>}
      </div>
      <span className="dw-part-size">
        {formatBytes(part.size_bytes)}
        {part.free_bytes != null && (
          <span className="dw-part-free"> · {formatBytes(part.free_bytes)} free</span>
        )}
      </span>
      <div className="dw-part-actions">
        <button
          className="dw-btn-ghost"
          onClick={() =>
            onScan(
              {
                uri: disk.id,
                label,
                detail: `${fs} · ${disk.display_name}`,
                partition: part.index,
              },
              false,
            )
          }
        >
          Quick scan
        </button>
        <button
          className="dw-btn-outline dw-sm"
          onClick={() =>
            onScan(
              {
                uri: disk.id,
                label,
                detail: `${fs} · ${disk.display_name}`,
                partition: part.index,
              },
              true,
            )
          }
        >
          <ScanSearchIcon size={14} />
          Deep scan
        </button>
      </div>
    </div>
  );
}

function DiskUsageBar({ disk }: { disk: DiskInfo }) {
  if (disk.free_bytes == null || disk.size_bytes === 0) return null;
  const used = Math.max(0, disk.size_bytes - disk.free_bytes);
  const pct = Math.min(100, Math.round((used / disk.size_bytes) * 100));
  return (
    <span className="dw-usage">
      <span className="dw-usage-track" role="presentation">
        <span
          className={`dw-usage-fill${pct > 90 ? " dw-usage-critical" : ""}`}
          style={{ width: `${pct}%` }}
        />
      </span>
      <span className="dw-usage-text">
        {formatBytes(disk.free_bytes)} free of {formatBytes(disk.size_bytes)}
      </span>
    </span>
  );
}

function DiskCard({ disk, onScan }: { disk: DiskInfo; onScan: Props["onScan"] }) {
  const [open, setOpen] = useState(false);
  const hasParts = disk.partitions.length > 0;

  return (
    <div className={`dw-disk-card${open ? " dw-open" : ""}`}>
      <button
        className="dw-disk-head"
        onClick={() => hasParts && setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="dw-disk-glyph">
          <DiskGlyph disk={disk} />
        </span>
        <span className="dw-disk-id">
          <span className="dw-disk-name dw-selectable-text">
            {disk.display_name}
            {disk.is_system && <span className="dw-tag dw-tag-amber">System</span>}
            {disk.is_removable && <span className="dw-tag dw-tag-blue">Removable</span>}
          </span>
          <span className="dw-disk-meta">
            {formatBytes(disk.size_bytes)} · {busLabel(disk.bus_type)}
            {hasParts && ` · ${disk.partitions.length} partition${disk.partitions.length === 1 ? "" : "s"}`}
          </span>
          <DiskUsageBar disk={disk} />
        </span>
        {hasParts && (
          <span className={`dw-disk-caret${open ? " dw-open" : ""}`}>
            <ChevronRightIcon size={15} />
          </span>
        )}
      </button>

      <div className="dw-disk-actions">
        <button
          className="dw-btn-outline dw-sm"
          onClick={() =>
            onScan(
              { uri: disk.id, label: disk.display_name, detail: `Whole disk · ${busLabel(disk.bus_type)}`, partition: null },
              false,
            )
          }
        >
          <PlayIcon size={14} />
          Quick scan
        </button>
        <button
          className="dw-btn-ink dw-sm"
          onClick={() =>
            onScan(
              { uri: disk.id, label: disk.display_name, detail: `Whole disk · ${busLabel(disk.bus_type)}`, partition: null },
              true,
            )
          }
        >
          <ScanSearchIcon size={15} />
          Deep scan
        </button>
      </div>

      {open && hasParts && (
        <div className="dw-part-list dw-fade-in">
          {disk.partitions.map((p) => (
            <PartitionRow key={p.index} part={p} disk={disk} onScan={onScan} />
          ))}
        </div>
      )}
    </div>
  );
}

export function SelectSource({ onScan, onResume }: Props) {
  const disks = useAppStore((s) => s.disks);
  const loading = useAppStore((s) => s.disksLoading);
  const error = useAppStore((s) => s.disksError);
  const saved = useAppStore((s) => s.savedScans);
  const savedLoading = useAppStore((s) => s.savedLoading);
  const [picking, setPicking] = useState(false);
  const [imgError, setImgError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  // The hidden input exists for the browser mock (real Tauri uses the
  // native dialog). Chosen images become file://-style paths for display.
  async function openImageDialog() {
    setPicking(true);
    setImgError(null);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const chosen = await open({
        title: "Open a disk image",
        multiple: false,
        filters: [
          { name: "Disk images", extensions: ["img", "dd", "raw", "vhd", "vhdx", "vmdk", "e01", "bin"] },
        ],
      });
      if (typeof chosen === "string" && chosen.length > 0) {
        onScan(
          { uri: chosen, label: chosen.split(/[\\/]/).pop() ?? chosen, detail: "Disk image file", partition: null },
          true,
        );
      }
    } catch {
      // Mock/browser fallback: the hidden file input path.
      fileRef.current?.click();
    } finally {
      setPicking(false);
    }
  }

  function onMockFileChosen(e: React.ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0];
    if (!f) return;
    onScan(
      { uri: `C:\\images\\${f.name}`, label: f.name, detail: "Disk image file", partition: null },
      true,
    );
    e.target.value = "";
  }

  return (
    <div className="dw-source-page">
      <div className="dw-source-hero">
        <div className="dw-source-hero-badge">
          <WandIcon size={15} />
          Free & open-source recovery
        </div>
        <h1 className="dw-source-title">Recover your files</h1>
        <p className="dw-source-sub">
          Pick the drive, partition or disk image where your files were lost.
          Quick scans read the filesystem's own records; deep scans also carve
          unallocated space byte-by-byte.
        </p>
        <div className="dw-trust-row">
          {TRUST.map((t) => (
            <span key={t} className="dw-trust-item">
              <span className="dw-trust-check" aria-hidden />
              {t}
            </span>
          ))}
        </div>
      </div>

      {error && (
        <div className="dw-source-error" role="alert">
          {error}
        </div>
      )}

      <section className="dw-source-section">
        <div className="dw-source-section-head">
          <h2>Drives</h2>
          <RefreshButton />
        </div>

        {loading ? (
          <div className="dw-disk-skeletons">
            {[0, 1, 2].map((i) => (
              <div key={i} className="dw-skeleton dw-disk-skel" />
            ))}
          </div>
        ) : disks.length === 0 ? (
          <div className="dw-source-empty">
            <HardDriveIcon size={26} />
            <p className="dw-source-empty-title">No drives appeared</p>
            <p className="dw-source-empty-sub">
              On Windows, run Data Wizard as administrator for full disk access —
              or open a disk image below instead.
            </p>
          </div>
        ) : (
          <div className="dw-disk-list">
            {disks.map((d) => (
              <DiskCard key={d.id} disk={d} onScan={onScan} />
            ))}
          </div>
        )}
      </section>

      <div className="dw-source-duo">
        <section className="dw-source-section dw-image-card">
          <div className="dw-source-section-head">
            <h2>Disk image</h2>
          </div>
          <button className="dw-image-cta" onClick={openImageDialog} disabled={picking}>
            <span className="dw-image-cta-icon">
              <DatabaseIcon size={20} />
            </span>
            <span className="dw-image-cta-text">
              <span className="dw-image-cta-title">Open a disk image</span>
              <span className="dw-image-cta-sub">RAW · IMG · DD · VHD · VHDX · VMDK · E01</span>
            </span>
          </button>
          {imgError && <p className="dw-image-error">{imgError}</p>}
          <input
            ref={fileRef}
            type="file"
            accept=".img,.dd,.raw,.vhd,.bin"
            style={{ display: "none" }}
            onChange={onMockFileChosen}
            aria-label="Disk image file"
          />
          <p className="dw-image-note">
            Scanning an image is the safest first step: the original media stays
            untouched while you verify what can be recovered.
          </p>
        </section>

        <section className="dw-source-section">
          <div className="dw-source-section-head">
            <h2>Saved scans</h2>
          </div>
          {savedLoading ? (
            <div className="dw-skeleton dw-saved-skel" />
          ) : saved.length === 0 ? (
            <div className="dw-saved-empty">
              <HistoryIcon size={20} />
              <p>No saved scans yet. Finish a scan and save it to resume later.</p>
            </div>
          ) : (
            <ul className="dw-saved-list">
              {saved.slice(0, 4).map((s) => (
                <li key={s.scan_id} className="dw-saved-row">
                  <span className="dw-saved-glyph">
                    <ClockIcon size={15} />
                  </span>
                  <span className="dw-saved-id">
                    <span className="dw-saved-source dw-selectable-text" title={s.source_uri}>
                      {s.source_uri.split(/[\\/]/).pop() ?? s.source_uri}
                    </span>
                    <span className="dw-saved-meta">
                      {s.mode === "deep" ? "Deep" : "Quick"} · {s.entries.toLocaleString()} files · {timeAgo(s.created_at)}
                    </span>
                  </span>
                  <button className="dw-btn-ghost" onClick={() => onResume(s)}>
                    Resume
                    <ChevronRightIcon size={13} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}

function RefreshButton() {
  const setDisks = useAppStore((s) => s.setDisks);
  const setLoading = useAppStore((s) => s.setDisksLoading);
  const setError = useAppStore((s) => s.setDisksError);
  const loading = useAppStore((s) => s.disksLoading);
  const [spinning, setSpinning] = useState(false);

  async function refresh() {
    setLoading(true);
    setError(null);
    setSpinning(true);
    try {
      const res = await invoke<{ disks: DiskInfo[] }>("list_disks", { req: {} });
      setDisks(res.disks ?? []);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      window.setTimeout(() => setSpinning(false), 350);
    }
  }

  return (
    <button className="dw-btn-ghost" onClick={refresh} disabled={loading}>
      <RefreshIcon size={14} className={spinning ? "dw-spin" : undefined} />
      {loading ? "Refreshing…" : "Refresh"}
    </button>
  );
}
