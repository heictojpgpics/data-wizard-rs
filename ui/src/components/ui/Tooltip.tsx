/**
 * Tooltip — the delayed token-driven hint layer. One singleton tooltip
 * element is mounted at the app root; any element can opt in with
 * `data-tip="..."`. Delay 450ms show / 80ms hide, mirror of the EaseUS
 * CToolTipController rhythm.
 *
 * Usage: <button data-tip="Refresh drives">…</button> — nothing else.
 * Rich content is not needed anywhere in the app so far (deliberate).
 */

import { useEffect, useRef, useState } from "react";

const SHOW_DELAY = 450;
const HIDE_DELAY = 80;
const MARGIN = 8;

export function Tooltip() {
  const [tip, setTip] = useState<string | null>(null);
  const [pos, setPos] = useState<{ x: number; y: number; above: boolean } | null>(null);
  const showTimer = useRef<number | null>(null);
  const hideTimer = useRef<number | null>(null);

  useEffect(() => {
    const clearTimers = () => {
      if (showTimer.current != null) window.clearTimeout(showTimer.current);
      if (hideTimer.current != null) window.clearTimeout(hideTimer.current);
      showTimer.current = null;
      hideTimer.current = null;
    };

    const hide = () => {
      clearTimers();
      hideTimer.current = window.setTimeout(() => setTip(null), HIDE_DELAY);
    };

    const onOver = (e: MouseEvent) => {
      const target = (e.target as HTMLElement | null)?.closest?.("[data-tip]") as
        | HTMLElement
        | null;
      if (!target) {
        if (tip != null) hide();
        return;
      }
      const text = target.dataset.tip;
      if (!text) return;
      clearTimers();
      showTimer.current = window.setTimeout(() => {
        const r = target.getBoundingClientRect();
        setTip(text);
        // Place above the element when there is room, else below.
        const above = r.top > 48;
        setPos({
          x: Math.min(
            Math.max(MARGIN, r.left + r.width / 2),
            window.innerWidth - MARGIN,
          ),
          y: above ? r.top - 6 : r.bottom + 6,
          above,
        });
      }, SHOW_DELAY);
    };

    const onOut = (e: MouseEvent) => {
      const target = (e.target as HTMLElement | null)?.closest?.("[data-tip]");
      if (target) hide();
    };

    const onDown = () => hide();

    window.addEventListener("mouseover", onOver, { passive: true });
    window.addEventListener("mouseout", onOut, { passive: true });
    window.addEventListener("mousedown", onDown, { passive: true });
    window.addEventListener("blur", onDown);
    return () => {
      window.removeEventListener("mouseover", onOver);
      window.removeEventListener("mouseout", onOut);
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("blur", onDown);
      clearTimers();
    };
  }, [tip]);

  if (tip == null || pos == null) return <div className="dw-tip" role="tooltip" aria-hidden />;

  return (
    <div
      className="dw-tip dw-open"
      role="tooltip"
      style={{
        left: pos.x,
        top: pos.y,
        transform: pos.above ? "translate(-50%, -100%)" : "translate(-50%, 0)",
      }}
    >
      {tip}
    </div>
  );
}
