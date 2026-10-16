/**
 * Toast — one ephemeral feedback bubble. The parent mounts one
 * instance per toast (keyed by id); the instance owns its lifecycle:
 * after the dwell it flips its closing flag, then asks the store to
 * drop the toast after the exit fade. All setState calls happen
 * inside timeout callbacks (async), never synchronously in effects.
 */
import { useEffect, useRef, useState } from "react";
import { CheckIcon, CircleXIcon, InfoIcon } from "../lib/Icon";
import { useAppStore } from "../state/app";

const DWELL_MS = 4200;
const EXIT_MS = 180;

export function Toasts() {
  const toast = useAppStore((s) => s.toast);
  const setToast = useAppStore((s) => s.setToast);
  const [closing, setClosing] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    timer.current = window.setTimeout(() => {
      // Phase 1: flip the closing attribute (the CSS fade-out runs).
      setClosing(true);
      // Phase 2: drop the toast after the exit transition.
      timer.current = window.setTimeout(() => setToast(null), EXIT_MS);
    }, DWELL_MS);
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [setToast]);

  if (!toast) return null;

  return (
    <div className={`dw-toast-zone${closing ? " dw-closing" : ""}`} role="status">
      <div className={`dw-toast dw-${toast.kind}`}>
        <span className="dw-toast-icon">
          {toast.kind === "ok" ? <CheckIcon size={14} /> : toast.kind === "err" ? <CircleXIcon size={14} /> : <InfoIcon size={14} />}
        </span>
        <span className="dw-toast-text dw-selectable-text">{toast.text}</span>
      </div>
    </div>
  );
}
