/**
 * ConfirmDialog — the DrwMessageBox pattern: icon + title + body +
 * explicit primary/secondary CTAs. Every destructive action routes
 * through here (stop scan, exit during scan, cancel recovery). The
 * danger variant tints the primary CTA and the icon.
 */

import { useEffect, useRef, type ReactNode } from "react";
import { AlertTriangleIcon, CircleCheckIcon, InfoIcon, CircleXIcon } from "../../lib/Icon";

export interface ConfirmSpec {
  title: string;
  body: ReactNode;
  confirmLabel: string;
  cancelLabel?: string;
  tone?: "danger" | "warn" | "ok" | "info" | "error";
  /** Optional checkbox ("Don't show this again" pattern). */
  checkboxLabel?: string;
  onConfirm: (opts: { dontAskAgain: boolean }) => void;
  onCancel?: () => void;
}
export function ConfirmDialog({
  spec,
  onClose,
}: {
  spec: ConfirmSpec;
  onClose: () => void;
}) {
  const confirmRef = useRef<HTMLButtonElement>(null);
  const checkRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    confirmRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const tone = spec.tone ?? "warn";
  const Icon =
    tone === "danger" || tone === "warn"
      ? AlertTriangleIcon
      : tone === "ok"
        ? CircleCheckIcon
        : tone === "error"
          ? CircleXIcon
          : InfoIcon;

  return (
    <div className="dw-scrim dw-dialog-scrim" role="presentation">
      <div
        className="dw-dialog dw-confirm"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="dw-confirm-title"
        aria-describedby="dw-confirm-body"
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            spec.onConfirm({ dontAskAgain: checkRef.current?.checked ?? false });
            onClose();
          }
        }}
      >
        <div className={`dw-confirm-icon dw-tone-${tone}`}>
          <Icon size={26} />
        </div>
        <h2 id="dw-confirm-title">{spec.title}</h2>
        <div id="dw-confirm-body" className="dw-confirm-body">
          {spec.body}
        </div>
        {spec.checkboxLabel ? (
          <label className="dw-confirm-check">
            <input ref={checkRef} type="checkbox" className="dw-check" />
            <span>{spec.checkboxLabel}</span>
          </label>
        ) : null}
        <div className="dw-confirm-actions">
          <button
            type="button"
            className="dw-btn-outline"
            onClick={() => {
              spec.onCancel?.();
              onClose();
            }}
          >
            {spec.cancelLabel ?? "Cancel"}
          </button>
          <button
            ref={confirmRef}
            type="button"
            className={`dw-btn-ink ${tone === "danger" ? "dw-danger" : ""}`}
            onClick={() => {
              spec.onConfirm({ dontAskAgain: checkRef.current?.checked ?? false });
              onClose();
            }}
          >
            {spec.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
