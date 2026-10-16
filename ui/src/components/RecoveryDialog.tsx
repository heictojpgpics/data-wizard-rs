/**
 * Recovery dialog — the recovery flow's control room: destination
 * picker with a different-drive warning, collision policy radio
 * cards with plain-language hints, live per-file progress, and the
 * end-of-run summary (successes, partials, failures, open-folder).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke, listen } from "../lib/ipc";
import type {
  FileRecoveryOutcome,
  RecoveryProgress,
} from "../bindings";
import {
  AlertTriangleIcon,
  ArrowRightIcon,
  CheckIcon,
  CircleCheckIcon,
  FolderOpenIcon,
  InfoIcon,
  XIcon,
} from "../lib/Icon";
import { formatBytes, formatCount } from "../lib/format";
import { pickDestination } from "../inspector/FileInspector";
import { useAppStore } from "../state/app";

type Phase = "setup" | "running" | "summary" | "error";

const POLICIES = [
  {
    id: "keep_both",
    title: "Keep both",
    hint: "A recovering file that already exists lands as “name (2).ext”. Nothing is ever replaced.",
  },
  {
    id: "overwrite",
    title: "Overwrite",
    hint: "Files with the same name in the destination are replaced. Faster, but destroys older copies.",
  },
  {
    id: "error",
    title: "Skip collisions",
    hint: "Name collisions are reported as failures and left untouched — you resolve them by hand.",
  },
] as const;

export function RecoveryDialog() {
  const open = true; // the parent mounts this dialog only while open
  const setOpen = useAppStore((s) => s.setRecoveryOpen);
  const rows = useAppStore((s) => s.rows);
  const selected = useAppStore((s) => s.selected);
  const scanId = useAppStore((s) => s.scanId);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const setToast = useAppStore((s) => s.setToast);

  const [destination, setDestination] = useState<string | null>(null);
  const [policy, setPolicy] = useState<(typeof POLICIES)[number]["id"]>("keep_both");
  const [phase, setPhase] = useState<Phase>("setup");
  const [progress, setProgress] = useState<RecoveryProgress | null>(null);
  const [stream, setStream] = useState<{ name: string; ok: boolean; note: string }[]>([]);
  const [outcomes, setOutcomes] = useState<FileRecoveryOutcome[] | null>(null);
  const [summary, setSummary] = useState<{
    recovered: number;
    failed: number;
    bytes: number;
    warnings: string[];
    errors: string[];
  } | null>(null);
  const [errText, setErrText] = useState<string | null>(null);
  const unlistenRef = useRef<null | (() => void)>(null);

  // CI tour hook: the tour driver fills the destination without the
  // native dialog (dw-tour-destination event).
  useEffect(() => {
    const onTourDest = (e: Event) => {
      const detail = (e as CustomEvent<{ path: string }>).detail;
      if (detail?.path) setDestination(detail.path);
    };
    window.addEventListener("dw-tour-destination", onTourDest);
    return () => window.removeEventListener("dw-tour-destination", onTourDest);
  }, []);

  // The progress event listener lives for the dialog's lifetime.
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    void listen<RecoveryProgress>("recovery:progress", (p) => {
      if (disposed) return;
      if (scanId && p.scan_id !== scanId) return;
      setProgress(p);
      setStream((s) => [
        ...s.slice(-24),
        { name: p.current_file, ok: true, note: "" },
      ]);
    }).then((un) => {
      if (disposed) un();
      else unlistenRef.current = un;
    });
    return () => {
      disposed = true;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, [open, scanId]);

  const selectedRows = useMemo(
    () => rows.filter((r) => selected.has(r.id)),
    [rows, selected],
  );
  const totalBytes = selectedRows.reduce((a, r) => a + r.size_bytes, 0);
  const okCount = outcomes?.filter((o) => o.ok).length ?? 0;
  const failCount = outcomes?.filter((o) => !o.ok).length ?? 0;

  if (!open || selectedRows.length === 0) return null;

  async function chooseDestination() {
    const dest = await pickDestination();
    if (dest) setDestination(dest);
  }

  async function run() {
    if (!destination || !scanId) return;
    setPhase("running");
    try {
      const res = await invoke<{
        recovered: number;
        failed: number;
        bytes_written: number;
        results: FileRecoveryOutcome[];
        warnings: string[];
        errors: string[];
      }>("recover_files", {
        req: {
          scan_id: scanId,
          entry_ids: selectedRows.map((r) => r.id),
          destination_dir: destination,
          collision_policy: policy,
        },
      });
      setOutcomes(res.results);
      setSummary({
        recovered: res.recovered,
        failed: res.failed,
        bytes: res.bytes_written,
        warnings: res.warnings,
        errors: res.errors,
      });
      setPhase("summary");
    } catch (e) {
      setErrText(String(e));
      setPhase("error");
    }
  }

  function finish() {
    setOpen(false);
    clearSelection();
    if (summary && summary.recovered > 0) {
      setToast({
        text: `Recovered ${formatCount(summary.recovered)} files (${formatBytes(summary.bytes)}) to ${destination}`,
        kind: "ok",
      });
    }
  }

  return (
    <div className="dw-scrim" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget && phase !== "running") setOpen(false); }}>
      <div
        className="dw-dialog dw-recovery-dialog dw-pop-in"
        role="dialog"
        aria-modal="true"
        aria-label="Recover files"
      >
        <div className="dw-dialog-head">
          <h2>
            {phase === "setup" && "Recover files"}
            {phase === "running" && "Recovering…"}
            {phase === "summary" && "Recovery complete"}
            {phase === "error" && "Recovery failed"}
          </h2>
          {phase !== "running" && (
            <button className="dw-btn-ghost dw-icon" aria-label="Close" onClick={() => (phase === "summary" ? finish() : setOpen(false))}>
              <XIcon size={15} />
            </button>
          )}
        </div>

        {/* ── Setup ─────────────────────────────────────────────────── */}
        {phase === "setup" && (
          <>
            <div className="dw-dialog-body">
              <div className="dw-rc-dest" onClick={() => void chooseDestination()} role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter") void chooseDestination(); }}>
                <span className="dw-rc-dest-icon">
                  <FolderOpenIcon size={18} />
                </span>
                <span className="dw-rc-dest-text">
                  <span className="dw-rc-dest-label">Recover to</span>
                  <span className="dw-rc-dest-path dw-selectable-text">
                    {destination ?? "Choose a destination folder…"}
                  </span>
                </span>
                <span className="dw-rc-dest-change">Change</span>
              </div>

              <div className="dw-rc-warn">
                <AlertTriangleIcon size={14} />
                Recover to a <strong>different drive</strong> than the source whenever
                possible — writing to the same drive can overwrite the very files
                you are recovering.
              </div>

              <div className="dw-rc-policy">
                <span className="dw-rc-policy-label">When a file with the same name already exists</span>
                <div className="dw-rc-policy-cards">
                  {POLICIES.map((p) => (
                    <button
                      key={p.id}
                      className={`dw-rc-policy-card${policy === p.id ? " dw-active" : ""}`}
                      onClick={() => setPolicy(p.id)}
                      role="radio"
                      aria-checked={policy === p.id}
                    >
                      <span className="dw-rc-policy-radio" aria-hidden />
                      <span className="dw-rc-policy-body">
                        <span className="dw-rc-policy-title">{p.title}</span>
                        <span className="dw-rc-policy-hint">{p.hint}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="dw-dialog-foot">
              <div className="dw-rc-summary-line">
                {formatCount(selectedRows.length)} files · {formatBytes(totalBytes)}
              </div>
              <div className="dw-dialog-foot-actions">
                <button className="dw-btn-outline" onClick={() => setOpen(false)}>
                  Cancel
                </button>
                <button className="dw-btn-ink dw-lg" disabled={!destination} onClick={() => void run()}>
                  Recover {formatCount(selectedRows.length)} files
                  <ArrowRightIcon size={15} />
                </button>
              </div>
            </div>
          </>
        )}

        {/* ── Running ───────────────────────────────────────────────── */}
        {phase === "running" && (
          <div className="dw-dialog-body dw-rc-running">
            <div className="dw-rc-progress-num">
              {progress ? `${progress.done} / ${progress.total}` : `0 / ${selectedRows.length}`}
            </div>
            <div className="dw-track dw-lg">
              <div
                className="dw-track-fill dw-active"
                style={{
                  width: progress && progress.total > 0
                    ? `${Math.max(3, (progress.done / progress.total) * 100)}%`
                    : "3%",
                }}
              />
            </div>
            <div className="dw-rc-progress-file dw-selectable-text">
              {progress?.current_file ?? "Preparing…"}
            </div>
            <div className="dw-rc-progress-bytes">
              {progress ? `${formatBytes(progress.bytes_written)} written` : ""}
            </div>
            <div className="dw-rc-stream" aria-live="polite">
              {stream.length === 0 ? (
                <>
                  <div className="dw-skeleton dw-stream-skel" />
                  <div className="dw-skeleton dw-stream-skel" />
                  <div className="dw-skeleton dw-stream-skel" />
                </>
              ) : (
                stream.slice(-6).map((s, i) => (
                  <div key={`${s.name}-${i}`} className={`dw-rc-stream-row${s.ok ? "" : " dw-fail"}`}>
                    <span className="dw-rc-stream-dot" aria-hidden />
                    <span className="dw-rc-stream-name dw-selectable-text">{s.name}</span>
                    <span className="dw-rc-stream-bytes">{s.ok ? "written" : s.note}</span>
                  </div>
                ))
              )}
            </div>
            <p className="dw-rc-running-note">
              Files are written as temporary copies, flushed, then published —
              an interrupted recovery never leaves half-written files behind.
            </p>
          </div>
        )}

        {/* ── Summary ───────────────────────────────────────────────── */}
        {phase === "summary" && summary && (
          <>
            <div className="dw-dialog-body">
              <div className="dw-rc-result-hero">
                <span className={`dw-rc-result-icon${summary.failed > 0 ? " dw-mixed" : ""}`}>
                  <CircleCheckIcon size={26} />
                </span>
                <div className="dw-rc-result-text">
                  <h3>
                    {summary.failed === 0
                      ? `All ${formatCount(summary.recovered)} files recovered`
                      : `${formatCount(summary.recovered)} recovered · ${formatCount(summary.failed)} failed`}
                  </h3>
                  <p>
                    {formatBytes(summary.bytes)} written to
                    <span className="dw-rc-dest-inline dw-selectable-text"> {destination}</span>
                  </p>
                </div>
              </div>

              {summary.warnings.length > 0 && (
                <div className="dw-rc-notice dw-warn">
                  <AlertTriangleIcon size={14} />
                  <div>
                    {summary.warnings.map((w, i) => (
                      <p key={i}>{w}</p>
                    ))}
                  </div>
                </div>
              )}
              {summary.errors.length > 0 && (
                <div className="dw-rc-notice dw-err">
                  <XIcon size={14} />
                  <div>
                    {summary.errors.map((e, i) => (
                      <p key={i}>{e}</p>
                    ))}
                  </div>
                </div>
              )}
              {summary.warnings.length === 0 && summary.errors.length === 0 && (
                <div className="dw-rc-notice dw-ok">
                  <CheckIcon size={14} />
                  <p>Every file passed write + flush verification. The originals on the source were never touched.</p>
                </div>
              )}
            </div>
            <div className="dw-dialog-foot">
              <div className="dw-rc-summary-line">
                {okCount > 0 && `${formatCount(okCount)} ok`}
                {failCount > 0 && ` · ${formatCount(failCount)} failed`}
              </div>
              <div className="dw-dialog-foot-actions">
                <button className="dw-btn-outline" onClick={() => setOpen(false)}>
                  Back to results
                </button>
                <button className="dw-btn-ink dw-lg" onClick={finish}>
                  Done
                </button>
              </div>
            </div>
          </>
        )}

        {/* ── Error ─────────────────────────────────────────────────── */}
        {phase === "error" && (
          <div className="dw-dialog-body">
            <div className="dw-rc-notice dw-err">
              <AlertTriangleIcon size={14} />
              <p className="dw-selectable-text">{errText}</p>
            </div>
            <div className="dw-rc-notice">
              <InfoIcon size={14} />
              <p>Nothing was half-written: each file publishes only after a full write + flush.</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
