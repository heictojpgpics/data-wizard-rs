/**
 * Toasts — the notification stack (EaseUS notify-center role, docked
 * bottom-right). Multi-toast, auto-dismiss, enter/exit motion.
 */

import { AlertTriangleIcon, CircleCheckIcon, CircleXIcon, InfoIcon, XIcon } from "../lib/Icon";
import { useUiStore, type Toast } from "../state/ui";

function ToastCard({ t }: { t: Toast }) {
  const drop = useUiStore((s) => s.beginCloseToast);
  const Icon =
    t.kind === "ok"
      ? CircleCheckIcon
      : t.kind === "err"
        ? CircleXIcon
        : t.kind === "warn"
          ? AlertTriangleIcon
          : InfoIcon;
  return (
    <div className={`dw-toast dw-toast-${t.kind}${t.closing ? " dw-closing" : ""}`} role="status">
      <span className="dw-toast-icon" aria-hidden>
        <Icon size={16} />
      </span>
      <span className="dw-toast-body">
        <span className="dw-toast-title">{t.title}</span>
        {t.body && <span className="dw-toast-text">{t.body}</span>}
      </span>
      <button className="dw-toast-x" aria-label="Dismiss" onClick={() => drop(t.id)}>
        <XIcon size={12} />
      </button>
    </div>
  );
}

export function Toasts() {
  const toasts = useUiStore((s) => s.toasts);
  if (toasts.length === 0) return null;
  return (
    <div className="dw-toasts" aria-live="polite">
      {toasts.map((t) => (
        <ToastCard key={t.id} t={t} />
      ))}
    </div>
  );
}
