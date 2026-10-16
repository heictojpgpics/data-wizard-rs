/**
 * DestinationDialog — the EaseUS ChooseDestinationPathDialog parity:
 * drive rows with capacity + free space, a folder browse button, live
 * free-space validation ("Need additional %1 of space"), and the
 * source-drive protection (block + explain — never silently allow).
 */

import { useMemo, useRef, useState } from "react";
import { FolderIcon, HardDriveIcon, UsbIcon, XIcon, AlertTriangleIcon } from "../lib/Icon";
import { formatBytes } from "../lib/format";
import { useDevicesStore } from "../state/devices";
import { useAppStore } from "../state/app";
import { useRecoveryStore, isSourceDrive } from "../state/recovery";
import { useResultsStore, selectionSummary } from "../state/results";
import { CapacityBar } from "./ui/CapacityBar";
import { SegmentedControl } from "./ui/Switch";
import type { CollisionPolicy } from "../bindings";

const webkitDirAttrs = {
  webkitdirectory: "",
  directory: "",
} as unknown as Record<string, string>;

export function DestinationDialog() {
  const stage = useRecoveryStore((s) => s.stage);
  const setDestination = useRecoveryStore((s) => s.setDestination);
  const collision = useRecoveryStore((s) => s.collision);
  const setCollision = useRecoveryStore((s) => s.setCollision);
  const run = useRecoveryStore((s) => s.run);
  const close = useRecoveryStore((s) => s.close);

  const disks = useDevicesStore((s) => s.disks);
  const source = useAppStore((s) => s.source);
  const selected = useResultsStore((s) => s.selected);
  const byId = useResultsStore((s) => s.byId);

  const [drive, setDrive] = useState<string | null>(null);
  const [subfolder, setSubfolder] = useState("");
  const [browsing, setBrowsing] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  /* Reset the pick when the dialog REOPENS — keyed remount pattern via
   * stage tracking (an effect that only runs on transition INTO the
   * destination stage; the react-hooks lint rule wants transitions,
   * not sync setState in effect bodies). */
  const [prevStage, setPrevStage] = useState(stage);
  if (stage !== prevStage) {
    setPrevStage(stage);
    if (stage === "destination") {
      setDrive(null);
      setSubfolder("");
    }
  }

  const sel = useMemo(
    () => selectionSummary(selected, byId),
    [selected, byId],
  );

  /* Destination rows: every partition with a drive letter (the user's
   * mental model of "drives"), labeled with parent disk info. */
  const rows = useMemo(() => {
    const out: {
      key: string;
      label: string;
      diskName: string;
      size: number;
      free: number | null;
      path: string;
      removable: boolean;
    }[] = [];
    for (const d of disks) {
      for (const p of d.partitions) {
        if (!p.drive_letter) continue;
        out.push({
          key: `${d.id}#${p.index}`,
          label: `${p.label ?? "Local disk"} (${p.drive_letter})`,
          diskName: d.display_name,
          size: p.size_bytes,
          free: p.free_bytes,
          path: `${p.drive_letter}:\\`,
          removable: d.is_removable,
        });
      }
    }
    return out.sort((a, b) => a.path.localeCompare(b.path));
  }, [disks]);

  const chosen = rows.find((r) => r.key === drive) ?? null;
  const destPath = chosen ? (subfolder ? `${chosen.path}${subfolder.replace(/^[/\\]+/, "")}` : chosen.path) : null;

  /* Validation: needs space + not the source drive. */
  const needMore =
    chosen && chosen.free != null && sel.bytes > chosen.free
      ? sel.bytes - (chosen.free ?? 0)
      : null;
  const sourceHit = destPath != null && isSourceDrive(destPath, source?.uri ?? null);

  const canRecover = chosen != null && destPath != null && needMore == null && !sourceHit;

  async function browse() {
    setBrowsing(true);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const dir = await open({
        title: "Choose a destination folder",
        multiple: false,
        directory: true,
        defaultPath: chosen?.path,
      });
      if (typeof dir === "string" && dir.length > 0) {
        // Normalize to drive + subfolder for the row model.
        const letter = dir.match(/^([A-Za-z]):/)?.[0];
        const row = letter ? rows.find((r) => r.path.startsWith(letter)) : null;
        if (row) {
          setDrive(row.key);
          setSubfolder(dir.slice(row.path.length));
        } else {
          setSubfolder(dir);
        }
      }
    } catch {
      fileRef.current?.click();
    } finally {
      setBrowsing(false);
    }
  }

  if (stage !== "destination") return null;

  return (
    <div className="dw-scrim dw-dialog-scrim" role="presentation">
      <div
        className="dw-dialog dw-dest"
        role="dialog"
        aria-modal="true"
        aria-labelledby="dw-dest-title"
      >
        <div className="dw-dialog-head">
          <h2 id="dw-dest-title">Where should the files go?</h2>
          <button className="dw-icon-btn" aria-label="Close" onClick={close}>
            <XIcon size={15} />
          </button>
        </div>

        <p className="dw-dest-sub">
          Recovering <b className="tnum">{sel.count.toLocaleString()}</b>{" "}
          {sel.count === 1 ? "file" : "files"} ·{" "}
          <b className="tnum">{formatBytes(sel.bytes)}</b>. Pick a destination with enough free
          space.
        </p>

        <div className="dw-dest-list">
          {rows.map((r) => {
            const isSource = isSourceDrive(r.path, source?.uri ?? null);
            const active = drive === r.key;
            return (
              <button
                key={r.key}
                className={`dw-dest-row${active ? " dw-active" : ""}${isSource ? " dw-blocked" : ""}`}
                onClick={() => !isSource && setDrive(active ? null : r.key)}
                aria-pressed={active}
                data-tip={isSource ? "This is the SOURCE drive — saving here could overwrite the very files you're recovering." : undefined}
              >
                <span className="dw-dest-icon">
                  {r.removable ? <UsbIcon size={20} /> : <HardDriveIcon size={20} />}
                </span>
                <span className="dw-dest-id">
                  <span className="dw-dest-name dw-selectable-text">{r.label}</span>
                  <span className="dw-dest-disk">{r.diskName}</span>
                  <CapacityBar total={r.size} free={r.free} />
                </span>
                <span className="dw-dest-free tnum">
                  {r.free != null ? `${formatBytes(r.free)} free` : "capacity only"}
                </span>
                {isSource && <span className="dw-dest-srcflag">Source!</span>}
              </button>
            );
          })}
        </div>

        <div className="dw-dest-browse">
          <button className="dw-btn-outline dw-sm" onClick={() => void browse()} disabled={browsing}>
            <FolderIcon size={14} />
            Choose a specific folder…
          </button>
          {subfolder && (
            <span className="dw-dest-path dw-selectable-text">
              {chosen?.path}
              {subfolder}
            </span>
          )}
          <input
            ref={fileRef}
            type="file"
            className="dw-visually-hidden"
            {...webkitDirAttrs}
            onChange={(e) => {
              const files = e.target.files;
              const first = files?.[0];
              if (first) {
                const rel = (first as { webkitRelativePath?: string }).webkitRelativePath ?? "";
                const folder = rel.split("/")[0] ?? "";
                const row = rows[0];
                if (row) {
                  setDrive(row.key);
                  setSubfolder(folder ? `Recovery\\${folder}` : "Recovery");
                }
              }
              e.target.value = "";
            }}
          />
        </div>

        {/* Collision policy */}
        <div className="dw-dest-policy">
          <span className="dw-dest-policy-label">If a file with the same name already exists:</span>
          <SegmentedControl<CollisionPolicy>
            ariaLabel="Collision policy"
            value={collision}
            onChange={setCollision}
            options={[
              { value: "keep_both", label: "Keep both" },
              { value: "overwrite", label: "Overwrite" },
              { value: "error", label: "Skip" },
            ]}
          />
        </div>

        {/* Validation messages (the EaseUS "Need additional %1" + source warning) */}
        {needMore != null && (
          <div className="dw-dest-warn" role="alert">
            <AlertTriangleIcon size={15} />
            Need additional <b className="tnum">{formatBytes(needMore)}</b> of space. Free up
            space or select another drive.
          </div>
        )}
        {sourceHit && (
          <div className="dw-dest-warn dw-danger" role="alert">
            <AlertTriangleIcon size={15} />
            <b>Source drive!</b> Saving recovered files onto the drive you lost them from can
            overwrite the data mid-recovery. Pick a different drive.
          </div>
        )}

        <div className="dw-dialog-actions">
          <button className="dw-btn-outline" onClick={close}>
            Cancel
          </button>
          <button
            className="dw-btn-ink dw-lg"
            disabled={!canRecover}
            onClick={() => {
              if (destPath) {
                setDestination(destPath, destPath);
                void run();
              }
            }}
          >
            Recover {sel.count.toLocaleString()} {sel.count === 1 ? "file" : "files"}
          </button>
        </div>
      </div>
    </div>
  );
}
