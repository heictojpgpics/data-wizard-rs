/**
 * Menu — the anchored popover surface (context menus, sort menu,
 * group-by menu, view switcher). Portal-free: renders inline with
 * position:fixed, flips to fit the viewport, closes on outside
 * pointer-down / Escape / scroll / resize, and manages its own focus.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";

export interface MenuOpenState {
  open: boolean;
  x: number;
  y: number;
}

export function useMenu() {
  const [state, setState] = useState<MenuOpenState>({ open: false, x: 0, y: 0 });
  const openAt = useCallback(
    (x: number, y: number) => setState({ open: true, x, y }),
    [],
  );
  const openFrom = useCallback((el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    setState({ open: true, x: r.left, y: r.bottom + 4 });
  }, []);
  const close = useCallback(() => setState((s) => (s.open ? { ...s, open: false } : s)), []);
  return { state, openAt, openFrom, close };
}

export interface MenuProps {
  state: MenuOpenState;
  onClose: () => void;
  children: ReactNode;
  /** Preferred horizontal alignment of the anchor point. */
  align?: "start" | "end";
  minWidth?: number;
  /** id wired to aria-haspopup + aria-expanded on the trigger. */
  id?: string;
}

export function Menu({ state, onClose, children, align = "start", minWidth = 190, id }: MenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  /* Position: fixed at the anchor, flipped to stay in the viewport. */
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    if (!state.open || !ref.current) return;
    const el = ref.current;
    const r = el.getBoundingClientRect();
    const pad = 8;
    let left = align === "end" ? state.x - r.width : state.x;
    if (left + r.width > window.innerWidth - pad) {
      left = window.innerWidth - pad - r.width;
    }
    if (left < pad) left = pad;
    let top = state.y;
    if (top + r.height > window.innerHeight - pad) {
      top = Math.max(pad, state.y - r.height - 8);
    }
    setPos({ left, top });
  }, [state, align]);

  /* Dismiss behaviors: outside pointerdown, Escape, scroll, resize. */
  useEffect(() => {
    if (!state.open) return;
    const onPointerDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    const onScroll = (e: Event) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    // capture-phase scroll listener catches scrolls in any scrollable.
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onClose);
    };
  }, [state.open, onClose]);

  if (!state.open) return null;

  return (
    <div
      ref={ref}
      id={id}
      className="dw-menu"
      role="menu"
      style={{
        position: "fixed",
        left: (pos?.left ?? state.x) + "px",
        top: (pos?.top ?? state.y) + "px",
        minWidth,
        visibility: pos == null ? "hidden" : undefined,
        zIndex: 240,
      }}
    >
      {children}
    </div>
  );
}

/* ── Item building blocks ─────────────────────────────────────────────── */

export function MenuItem({
  icon,
  label,
  hint,
  checked,
  danger,
  disabled,
  onSelect,
}: {
  icon?: ReactNode;
  label: string;
  hint?: string;
  checked?: boolean;
  danger?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role={checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={checked}
      disabled={disabled}
      className={[
        "dw-menu-item",
        danger ? "dw-danger" : "",
        checked ? "dw-checked" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={onSelect}
    >
      {icon}
      <span>{label}</span>
      {hint ? <span className="dw-kbd">{hint}</span> : null}
    </button>
  );
}

export function MenuSep() {
  return <div className="dw-menu-sep" role="separator" />;
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return <div className="dw-menu-label">{children}</div>;
}
