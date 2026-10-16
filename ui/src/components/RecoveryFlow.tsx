/**
 * RecoveryProgressDialog + RecoverySummary — the EaseUS
 * RecoverProgressDialog parity: per-file stream, counters, bytes,
 * cancel-confirm, then the celebratory completion card with the
 * failure list + reasons and "open location" affordance.
 */

import { useEffect } from "react";
import {
  CircleCheckIcon,
  CircleXIcon,
  AlertTriangleIcon,
  DownloadIcon,
  ExternalLinkIcon,
  XIcon,
} from "../lib/Icon";
import { formatBytes } from "../lib/format";
import {
  useRecoveryStore,
  recoverySummary,
  FAILURE_REASONS,
} from "../state/recovery";
import { useUiStore } from "../state/ui";
import { TypeIcon } from "./ui/TypeIcon";
import { Odometer } from "./ui/Odometer";

export function RecoveryFlow() {
  const stage = useRecoveryStore((s) => s.stage);
  if (stage === "running") return <RunningDialog />;
  if (stage === "summary") return <SummaryDialog />;
  return null;
}

function RunningDialog() {
  const done = useRecoveryStore((s) => s.done);
  const total = useRecoveryStore((s) => s.total);
  const currentFile = useRecoveryStore((s) => s.currentFile);
  const bytes = useRecoveryStore((s) => s.bytesWritten);
  const cancelRequested = useRecoveryStore((s) => s.cancelRequested);
  const requestCancel = useRecoveryStore((s) => s.requestCancel);
  const openConfirm = useUiStore((s) => s.openConfirm);

  const pct = total > 0 ? Math.min(100, (done / total) * 100) : 0;

  return (
    <div className="dw-scrim dw-dialog-scrim" role="presentation">
      <div className="dw-dialog dw-recprog" role="dialog" aria-modal="true" aria-labelledby="dw-rp-title">
        <h2 id="dw-rp-title">
          <DownloadIcon size={18} /> Recovering your files
        </h2>

        <div className="dw-rp-counters tnum">
          <span className="dw-rp-big">
            <Odometer value={done} /> <span className="dw-rp-of">/ {total.toLocaleString()}</span>
          </span>
          <span className="dw-rp-bytes">{formatBytes(bytes)} written</span>
        </div>

        <div className="dw-track dw-lg dw-track-elev">
          <div className="dw-track-fill dw-active" style={{ width: `${pct.toFixed(1)}%` }} />
        </div>

        <div className="dw-rp-current" key={currentFile}>
          <span className="dw-rp-current-label">Recovering:</span>
          <span className="dw-rp-current-file dw-selectable-text">{currentFile || "…"}</span>
        </div>

        <div className="dw-rp-actions">
          <button
            className="dw-btn-outline dw-danger"
            disabled={cancelRequested}
            onClick={() =>
              openConfirm({
                title: "Sure to stop recovering?",
                body: "The remaining files will not be recovered if you stop now. Files already written stay on disk — this only cancels what's left.",
                confirmLabel: "Stop recovering",
                cancelLabel: "Continue",
                tone: "danger",
                onConfirm: () => requestCancel(),
              })
            }
          >
            {cancelRequested ? "Stopping…" : "Stop"}
          </button>
        </div>
      </div>
    </div>
  );
}

function SummaryDialog() {
  const outcomes = useRecoveryStore((s) => s.outcomes);
  const warnings = useRecoveryStore((s) => s.warnings);
  const destination = useRecoveryStore((s) => s.destinationLabel);
  const close = useRecoveryStore((s) => s.close);

  const sum = recoverySummary(outcomes);
  const allOk = sum.failed === 0 && sum.ok > 0;

  useEffect(() => {
    if (allOk && sum.ok > 0) {
      /* The celebratory moment — subtle, no confetti dependency. */
      document.title = "Recovery complete — Data Wizard";
      return () => {
        document.title = "Data Wizard";
      };
    }
    return undefined;
  }, [allOk, sum.ok]);

  async function openLocation() {
    if (!destination) return;
    try {
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
      // destinationLabel holds the dir path; reveal a child when known.
      await revealItemInDir(destination);
    } catch {
      // opener plugin not present: copy the path for the user.
      void navigator.clipboard?.writeText(destination).catch(() => undefined);
      useUiStore.getState().pushToast({
        kind: "info",
        title: "Destination path copied",
        body: destination,
      });
    }
  }

  return (
    <div className="dw-scrim dw-dialog-scrim" role="presentation">
      <div
        className={`dw-dialog dw-recsum dw-pop-in${allOk ? " dw-ok" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="dw-rs-title"
      >
        <button className="dw-icon-btn dw-recsum-x" aria-label="Close" onClick={close}>
          <XIcon size={15} />
        </button>

        <span className={`dw-recsum-icon${allOk ? " dw-ok" : " dw-mixed"}`}>
          {allOk ? <CircleCheckIcon size={36} /> : <AlertTriangleIcon size={36} />}
        </span>
        <h2 id="dw-rs-title">{allOk ? "Recovery successful!" : "Recovery finished with issues"}</h2>

        <p className="dw-recsum-sub tnum">
          Recovered <b>{sum.ok.toLocaleString()}</b> {sum.ok === 1 ? "file" : "files"} (
          {formatBytes(sum.bytes)})
          {sum.failed > 0 && (
            <>
              {" "}
              · <b className="dw-recsum-failed">{sum.failed.toLocaleString()} failed</b>
            </>
          )}
        </p>

        {destination && (
          <div className="dw-recsum-dest">
            <span className="dw-recsum-dest-label">Recovered to:</span>
            <span className="dw-recsum-dest-path dw-selectable-text">{destination}</span>
            <button className="dw-btn-outline dw-sm" onClick={() => void openLocation()}>
              <ExternalLinkIcon size={13} />
              Open location
            </button>
          </div>
        )}

        {/* Failures (the EaseUS numbered-reasons panel). */}
        {sum.failedList.length > 0 && (
          <div className="dw-recsum-failures">
            <div className="dw-recsum-fail-head">
              <CircleXIcon size={14} />
              <span>
                {sum.failedList.length} {sum.failedList.length === 1 ? "file" : "files"} could not
                be recovered:
              </span>
            </div>
            <ul className="dw-recsum-fail-list">
              {sum.failedList.slice(0, 8).map((f) => (
                <li key={f.entry_id} className="dw-recsum-fail-row">
                  <TypeIcon name={f.name} extension={f.name.split(".").pop() ?? ""} size={18} />
                  <span className="dw-selectable-text">{f.name}</span>
                  <span className="dw-recsum-fail-why">{f.error ?? "unknown error"}</span>
                </li>
              ))}
              {sum.failedList.length > 8 && (
                <li className="dw-recsum-fail-more tnum">
                  + {sum.failedList.length - 8} more
                </li>
              )}
            </ul>
            <div className="dw-recsum-fail-reasons">
              <span className="dw-recsum-fail-reasons-label">Possible reasons:</span>
              <ol>
                {FAILURE_REASONS.map((r) => (
                  <li key={r}>{r}</li>
                ))}
              </ol>
            </div>
          </div>
        )}

        {warnings.length > 0 && (
          <div className="dw-recsum-warns">
            {warnings.slice(0, 3).map((w, i) => (
              <span key={i}>{w}</span>
            ))}
          </div>
        )}

        <div className="dw-recsum-actions">
          <button className="dw-btn-outline" onClick={close}>
            Back to results
          </button>
          <button className="dw-btn-ink dw-lg" onClick={close}>
            Done
          </button>
        </div>

        <p className="dw-recsum-footnote">
          Recover sooner for better chances — every write to the source drive lowers the odds for
          what's still lost. Keep recovered data somewhere safe and back it up.
        </p>
      </div>
    </div>
  );
}
