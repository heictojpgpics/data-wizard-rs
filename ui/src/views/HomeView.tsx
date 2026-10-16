/**
 * Home (source select) — the EaseUS DrwHomePage parity view.
 *
 *   hero            headline + subline + scan-mode toggle
 *   drive groups    Internal / External / Removable card list; each card
 *                   expands into partition rows; per-row Scan CTA
 *   quick targets   Recycle Bin · Specific folder · Disk image
 *   recent scans    resume chips (the "Guess you want" rail)
 *   troubleshooting can't-see-drive flyout (refresh, replug, elevation)
 *
 * Every scan entry point funnels into ONE handler that resolves the
 * source, wires ChosenSource, and hands off to the scan store.
 */

import { useRef, useState } from "react";
import type { DiskInfo, PartitionInfo, SavedScanSummary } from "../bindings";
import {
  ChevronRightIcon,
  ClockIcon,
  DiscIcon,
  FolderSearchIcon,
  HardDriveIcon,
  HistoryIcon,
  ImageImportIcon,
  InfoIcon,
  PlayIcon,
  RecycleBinIcon,
  RefreshIcon,
  ScanSearchIcon,
  SdCardIcon,
  UsbIcon,
  XIcon,
} from "../lib/Icon";
import { busLabel, formatBytes, timeAgo } from "../lib/format";
import { useDevicesStore } from "../state/devices";
import type { ChosenSource } from "../state/app";
import { useScanStore } from "../state/scan";
import { useUiStore } from "../state/ui";
import { CapacityBar } from "../components/ui/CapacityBar";
import { EmptyState } from "../components/ui/EmptyState";
import { Switch } from "../components/ui/Switch";

/* ── Drive glyph by kind ───────────────────────────────────────────────── */

function DriveGlyph({ disk }: { disk: DiskInfo }) {
  if (disk.bus_type === "Sd" || disk.bus_type === "Mmc") return <SdCardIcon size={22} />;
  if (disk.is_removable) return <UsbIcon size={22} />;
  if (disk.bus_type === "Nvme") return <DiscIcon size={22} />;
  return <HardDriveIcon size={22} />;
}

/* ── Partition row ─────────────────────────────────────────────────────── */

function PartitionRow({
  part,
  disk,
  onScan,
}: {
  part: PartitionInfo;
  disk: DiskInfo;
  onScan: (src: ChosenSource, deep: boolean) => void;
}) {
  const fs = part.fs_kind?.toUpperCase() ?? "UNKNOWN";
  const label = part.drive_letter
    ? `${part.label ?? part.drive_letter} (${part.drive_letter})`
    : part.label ?? `Partition ${part.index}`;

  return (
    <div className="dw-part-row">
      <span className="dw-part-id">
        <span className="dw-part-name dw-selectable-text" title={label}>
          {label}
        </span>
        <span className="dw-part-fs">{fs}</span>
        {part.is_system && <span className="dw-part-badge">System</span>}
      </span>
      <span className="dw-part-cap">
        <CapacityBar
          total={part.size_bytes}
          free={part.free_bytes}
          label={`${label} capacity`}
          showLabels
        />
      </span>
      <div className="dw-part-actions">
        <button
          className="dw-btn-ghost dw-sm"
          data-tip="Read the filesystem's own records — minutes"
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
          data-tip="Also carve unallocated space byte-by-byte"
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

/* ── Drive card ────────────────────────────────────────────────────────── */

function DriveCard({
  disk,
  onScan,
}: {
  disk: DiskInfo;
  onScan: (src: ChosenSource, deep: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const hasParts = disk.partitions.length > 0;

  return (
    <div className={`dw-disk-card${open ? " dw-open" : ""}`}>
      <div className="dw-disk-head">
        <button
          className="dw-disk-id"
          onClick={() => hasParts && setOpen((v) => !v)}
          aria-expanded={open}
          disabled={!hasParts}
        >
          <span className="dw-disk-glyph">
            <DriveGlyph disk={disk} />
          </span>
          <span className="dw-disk-info">
            <span className="dw-disk-name dw-selectable-text">
              {disk.display_name}
              {disk.is_system && <span className="dw-tag dw-tag-amber">System</span>}
              {disk.is_removable && <span className="dw-tag dw-tag-blue">Removable</span>}
            </span>
            <span className="dw-disk-meta">
              {formatBytes(disk.size_bytes)} · {busLabel(disk.bus_type)}
              {hasParts && ` · ${disk.partitions.length} partition${disk.partitions.length === 1 ? "" : "s"}`}
            </span>
            <CapacityBar
              total={disk.size_bytes}
              free={disk.free_bytes}
              label={`${disk.display_name} capacity`}
            />
          </span>
          {hasParts && (
            <span className={`dw-disk-caret${open ? " dw-open" : ""}`} aria-hidden>
              <ChevronRightIcon size={15} />
            </span>
          )}
        </button>

        <div className="dw-disk-actions">
          <button
            className="dw-btn-outline dw-sm"
            data-tip="Scan the whole disk, including lost partitions"
            onClick={() =>
              onScan(
                {
                  uri: disk.id,
                  label: disk.display_name,
                  detail: `Whole disk · ${busLabel(disk.bus_type)}`,
                  partition: null,
                },
                true,
              )
            }
          >
            <PlayIcon size={14} />
            Scan entire disk
          </button>
        </div>
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

/* ── Quick targets ─────────────────────────────────────────────────────── */

function QuickTargets({ onScan }: { onScan: (src: ChosenSource, deep: boolean) => void }) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [picking, setPicking] = useState<"folder" | "image" | null>(null);
  const toast = useUiStore((s) => s.pushToast);
  const disks = useDevicesStore((s) => s.disks);

  /** Folder scan: resolve the volume that holds the folder, scan it,
   *  and auto-filter results to the folder's path prefix. */
  async function pickFolder() {
    setPicking("folder");
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const chosen = await open({
        title: "Choose the folder your files were lost from",
        multiple: false,
        directory: true,
      });
      if (typeof chosen === "string" && chosen.length > 0) {
        await scanFolder(chosen);
      }
    } catch {
      fileRef.current?.click();
    } finally {
      setPicking(null);
    }
  }

  async function scanFolder(path: string) {
    // Find the disk+partition whose drive letter owns this folder.
    const letter = path.match(/^([A-Za-z]):/)?.[1]?.toUpperCase() ?? null;
    const norm = path.replace(/\\/g, "/").replace(/\/+$/, "");
    for (const d of disks) {
      const part = letter
        ? d.partitions.find((p) => p.drive_letter?.toUpperCase().startsWith(letter))
        : undefined;
      if (part && letter) {
        const prefix = norm.slice(norm.toUpperCase().indexOf(letter) + 2) || "/";
        onScan(
          {
            uri: d.id,
            label: `Folder: ${norm.split("/").pop() || norm}`,
            detail: `On ${part.label ?? letter}: (${letter}) · ${d.display_name}`,
            partition: part.index,
            pathPrefix: prefix,
          },
          true,
        );
        return;
      }
    }
    toast({
      kind: "warn",
      title: "Folder is not on a scannable volume",
      body: "Pick a folder on an internal or external drive you can see in the list above.",
    });
  }

  function onMockFolderChosen(e: React.ChangeEvent<HTMLInputElement>) {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    // Browser mock: pretend it was on the system volume.
    const name = files[0].webkitRelativePath?.split("/")[0] ?? files[0].name;
    const d = disks.find((x) => x.is_system) ?? disks[0];
    const part = d?.partitions.find((p) => p.drive_letter) ?? d?.partitions[0];
    if (d && part) {
      onScan(
        {
          uri: d.id,
          label: `Folder: ${name}`,
          detail: `On ${d.display_name}`,
          partition: part.index,
          pathPrefix: `/Users/Example/${name}`,
        },
        true,
      );
    }
    e.target.value = "";
  }

  async function pickImage() {
    setPicking("image");
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const chosen = await open({
        title: "Open a disk image",
        multiple: false,
        filters: [
          {
            name: "Disk images",
            extensions: ["vhd", "vhdx", "img", "dd", "raw", "vmdk", "e01", "bin", "iso"],
          },
        ],
      });
      if (typeof chosen === "string" && chosen.length > 0) {
        onScan(
          {
            uri: chosen,
            label: chosen.split(/[\\/]/).pop() ?? chosen,
            detail: "Disk image file",
            partition: null,
          },
          true,
        );
      }
    } catch {
      fileRef.current?.click();
    } finally {
      setPicking(null);
    }
  }

  function onMockFileChosen(e: React.ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0];
    if (!f) return;
    onScan(
      {
        uri: `C:\\images\\${f.name}`,
        label: f.name,
        detail: "Disk image file",
        partition: null,
      },
      true,
    );
    e.target.value = "";
  }

  /** Recycle Bin: scan the system volume, filter to $Recycle.Bin. */
  function scanRecycleBin() {
    const d = disks.find((x) => x.is_system) ?? disks.find((x) => x.partitions.some((p) => p.drive_letter));
    const part = d?.partitions.find((p) => p.drive_letter && p.fs_kind === "ntfs") ?? d?.partitions.find((p) => p.drive_letter);
    if (!d || !part) {
      toast({
        kind: "warn",
        title: "No Windows volume found",
        body: "The Recycle Bin lives on an NTFS system volume — none is visible right now.",
      });
      return;
    }
    onScan(
      {
        uri: d.id,
        label: "Recycle Bin",
        detail: `On ${part.drive_letter} · ${d.display_name}`,
        partition: part.index,
        pathPrefix: "/$Recycle.Bin",
      },
      true,
    );
  }

  return (
    <section className="dw-source-section dw-quick-targets">
      <div className="dw-source-section-head">
        <h2>Quick targets</h2>
      </div>
      <div className="dw-qt-grid">
        <button className="dw-qt-card" onClick={scanRecycleBin}>
          <span className="dw-qt-icon dw-qt-cyan">
            <RecycleBinIcon size={20} />
          </span>
          <span className="dw-qt-body">
            <span className="dw-qt-title">Recycle Bin</span>
            <span className="dw-qt-sub">Files you deleted that skipped the bin, or an emptied bin</span>
          </span>
        </button>

        <button className="dw-qt-card" onClick={pickFolder} disabled={picking === "folder"}>
          <span className="dw-qt-icon dw-qt-teal">
            <FolderSearchIcon size={20} />
          </span>
          <span className="dw-qt-body">
            <span className="dw-qt-title">Specific folder</span>
            <span className="dw-qt-sub">Scan just the folder your files lived in</span>
          </span>
        </button>

        <button className="dw-qt-card" onClick={pickImage} disabled={picking === "image"}>
          <span className="dw-qt-icon dw-qt-violet">
            <ImageImportIcon size={20} />
          </span>
          <span className="dw-qt-body">
            <span className="dw-qt-title">Disk image</span>
            <span className="dw-qt-sub">Scan a VHD, VHDX, raw or E01 copy of a drive</span>
          </span>
        </button>
      </div>
      {/* Browser-mock fallback inputs (hidden; native dialogs in Tauri). */}
      <input
        ref={fileRef}
        type="file"
        className="dw-visually-hidden"
        onChange={(e) => {
          // Route by which picker is active: folder mock uses the same
          // input via webkitdirectory fallback.
          if (picking === "folder") onMockFolderChosen(e);
          else onMockFileChosen(e);
        }}
        {...(picking === "folder" ? { webkitdirectory: "", directory: "" } : {})}
      />
    </section>
  );
}

/* ── Troubleshooting flyout ────────────────────────────────────────────── */

function TroubleshootFlyout({ onClose }: { onClose: () => void }) {
  const refresh = useDevicesStore((s) => s.refresh);
  return (
    <div className="dw-trouble-card">
      <div className="dw-trouble-head">
        <InfoIcon size={16} />
        <span>Can't see your drive?</span>
        <button className="dw-topbar-iconbtn dw-sm" aria-label="Close" onClick={onClose}>
          <XIcon size={14} />
        </button>
      </div>
      <ol className="dw-trouble-list">
        <li>
          <b>Refresh the list</b> — drives that were just plugged in appear automatically, but a
          manual refresh forces it now.
        </li>
        <li>
          <b>Replug the drive</b> — use a rear USB port directly on the PC (hubs can under-power
          spinning disks).
        </li>
        <li>
          <b>Run as administrator</b> — Windows only exposes raw disk handles to elevated apps.
          Right-click Data Wizard and choose <i>Run as administrator</i>.
        </li>
        <li>
          <b>Still nothing?</b> The drive may have failed at the controller level. A disk image
          taken on another machine can still be scanned here.
        </li>
      </ol>
      <button
        className="dw-btn-outline dw-sm dw-trouble-refresh"
        onClick={() => void refresh()}
      >
        <RefreshIcon size={14} />
        Refresh drives
      </button>
    </div>
  );
}

/* ── The view ──────────────────────────────────────────────────────────── */

export function HomeView() {
  const disks = useDevicesStore((s) => s.disks);
  const loading = useDevicesStore((s) => s.disksLoading);
  const error = useDevicesStore((s) => s.disksError);
  const saved = useDevicesStore((s) => s.savedScans);
  const savedLoading = useDevicesStore((s) => s.savedLoading);
  const refresh = useDevicesStore((s) => s.refresh);
  const deleteSaved = useDevicesStore((s) => s.deleteSaved);

  const [deepDefault, setDeepDefault] = useState(true);
  const [trouble, setTrouble] = useState(false);
  const scan = useScanStore();
  const toast = useUiStore((s) => s.pushToast);

  function onScan(src: ChosenSource, deep: boolean) {
    void scan
      .start({ source: src, deep })
      .catch((e) => {
        toast({ kind: "err", title: "Could not start the scan", body: String(e) });
      });
  }

  function onResume(s: SavedScanSummary) {
    const deep = s.mode === "deep";
    void scan
      .resume(
        s.scan_id,
        deep,
        {
          uri: s.source_uri,
          label: s.source_uri.split(/[\\/]/).pop() ?? s.source_uri,
          detail: `Resumed · ${s.entries} files found previously`,
          partition: null,
        },
      )
      .catch((e) => {
        toast({ kind: "err", title: "Could not resume this scan", body: String(e) });
      });
  }

  const internal = disks.filter((d) => !d.is_removable);
  const external = disks.filter((d) => d.is_removable);
  const recent = saved.slice(0, 4);

  return (
    <div className="dw-home">
      <div className="dw-home-scroll">
        {/* Hero */}
        <div className="dw-source-hero">
          <div className="dw-source-hero-badge">
            <ScanSearchIcon size={15} />
            Free & unlimited recovery
          </div>
          <h1 className="dw-source-title">Where did you lose your files?</h1>
          <p className="dw-source-sub">
            Pick a location to scan and Data Wizard will find the deleted or missing files on it.
            You can preview everything before recovering — and recover as much as you want, free.
          </p>
          <div className="dw-hero-controls">
            <Switch
              checked={deepDefault}
              onChange={setDeepDefault}
              label="Deep scan by default (finds more, takes longer)"
            />
          </div>
          <div className="dw-trust-row">
            <span className="dw-trust-item">
              <span className="dw-trust-check" aria-hidden />
              Read-only scanning
            </span>
            <span className="dw-trust-item">
              <span className="dw-trust-check" aria-hidden />
              Nothing is written to the source
            </span>
            <span className="dw-trust-item">
              <span className="dw-trust-check" aria-hidden />
              Unlimited recovery, no quota
            </span>
          </div>
        </div>

        {error && (
          <div className="dw-source-error" role="alert">
            {error}
            <button className="dw-btn-ghost dw-sm" onClick={() => void refresh()}>
              <RefreshIcon size={14} />
              Retry
            </button>
          </div>
        )}

        {/* Drives */}
        <section className="dw-source-section">
          <div className="dw-source-section-head">
            <h2>Drives</h2>
            <div className="dw-source-section-tools">
              <button
                className="dw-btn-ghost dw-sm"
                data-tip="Show the drive troubleshooting guide"
                onClick={() => setTrouble((v) => !v)}
                aria-expanded={trouble}
              >
                Can't find a location?
              </button>
              <button
                className="dw-btn-outline dw-sm"
                onClick={() => void refresh()}
                disabled={loading}
              >
                <RefreshIcon size={14} className={loading ? "dw-spin" : undefined} />
                Refresh
              </button>
            </div>
          </div>

          {trouble && <TroubleshootFlyout onClose={() => setTrouble(false)} />}

          {loading ? (
            <div className="dw-disk-skeletons">
              {[0, 1, 2].map((i) => (
                <div key={i} className="dw-skeleton dw-disk-skel" />
              ))}
            </div>
          ) : disks.length === 0 ? (
            <EmptyState
              icon={<HardDriveIcon size={30} />}
              title="No drives appeared"
              body={
                <>
                  Windows only exposes raw disk handles to <b>elevated</b> apps — run Data Wizard
                  as administrator, replug the drive, or{" "}
                  <b>open a disk image</b> instead below.
                </>
              }
              actions={
                <>
                  <button className="dw-btn-outline" onClick={() => void refresh()}>
                    <RefreshIcon size={14} />
                    Refresh drives
                  </button>
                  <button className="dw-btn-ghost" onClick={() => setTrouble(true)}>
                    Troubleshooting guide
                  </button>
                </>
              }
            />
          ) : (
            <div className="dw-drive-groups">
              {internal.length > 0 && (
                <DriveGroup
                  label={`Internal drives (${internal.length})`}
                  disks={internal}
                  onScan={onScan}
                />
              )}
              {external.length > 0 && (
                <DriveGroup
                  label={`External & removable (${external.length})`}
                  disks={external}
                  onScan={onScan}
                />
              )}
            </div>
          )}
        </section>

        <QuickTargets onScan={onScan} />

        {/* Recent scans */}
        {recent.length > 0 && (
          <section className="dw-source-section">
            <div className="dw-source-section-head">
              <h2>Recent scans</h2>
              <span className="dw-section-note">Continue where you left off</span>
            </div>
            <div className="dw-recent-grid">
              {recent.map((s) => (
                <div key={s.scan_id} className="dw-recent-chip">
                  <ClockIcon size={15} />
                  <span className="dw-recent-body">
                    <span className="dw-recent-title dw-selectable-text">
                      {s.source_uri.split(/[\\/]/).pop() ?? s.source_uri}
                    </span>
                    <span className="dw-recent-meta tnum">
                      {s.mode === "deep" ? "Deep" : "Quick"} · {s.entries.toLocaleString()} files ·{" "}
                      {timeAgo(s.created_at)}
                    </span>
                  </span>
                  <button
                    className="dw-btn-ghost dw-sm"
                    aria-label="Delete saved scan"
                    data-tip="Remove this saved scan"
                    onClick={() => void deleteSaved(s.scan_id)}
                  >
                    <XIcon size={13} />
                  </button>
                  <button className="dw-btn-outline dw-sm" onClick={() => onResume(s)}>
                    <HistoryIcon size={14} />
                    Resume
                  </button>
                </div>
              ))}
            </div>
          </section>
        )}

        {savedLoading && recent.length === 0 && (
          <div className="dw-skeleton dw-recent-skel" />
        )}

        {/* Footer note (EaseUS's reassurance footer, minus the upsell). */}
        <footer className="dw-home-footer">
          Deep scan reads every sector, including unallocated space, and reconstructs files by
          their signatures — the best chance for files lost long ago or after a format.
        </footer>
      </div>
    </div>
  );
}

function DriveGroup({
  label,
  disks,
  onScan,
}: {
  label: string;
  disks: DiskInfo[];
  onScan: (src: ChosenSource, deep: boolean) => void;
}) {
  return (
    <div className="dw-drive-group">
      <h3 className="dw-drive-group-label">{label}</h3>
      <div className="dw-disk-list">
        {disks.map((d) => (
          <DriveCard key={d.id} disk={d} onScan={onScan} />
        ))}
      </div>
    </div>
  );
}
