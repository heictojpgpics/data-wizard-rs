/**
 * Theme picker popover — the five palettes with live swatch previews.
 * Opens from the topbar palette button; closes on outside click or
 * Escape. The quick sun/moon toggle lives beside it in the topbar.
 */
import { useEffect, useRef, useState } from "react";
import { CheckIcon, PaletteIcon } from "../lib/Icon";
import { THEME_LIST, useTheme } from "../theme/useTheme";

export function ThemePicker() {
  const { theme, setTheme } = useTheme();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="dw-theme-picker" ref={rootRef}>
      <button
        className="dw-topbar-iconbtn"
        aria-label="Choose theme"
        title="Theme"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <PaletteIcon size={16} />
      </button>
      {open && (
        <div className="dw-theme-popover" role="menu" aria-label="Appearance">
          <div className="dw-theme-popover-title">Appearance</div>
          <div className="dw-theme-grid">
            {THEME_LIST.map((t) => (
              <button
                key={t.id}
                role="menuitemradio"
                aria-checked={theme === t.id}
                className={`dw-theme-card${theme === t.id ? " dw-active" : ""}`}
                onClick={() => {
                  setTheme(t.id);
                  setOpen(false);
                }}
              >
                <span className="dw-theme-swatch">
                  <span className="dw-theme-swatch-a" style={{ background: t.swatch[0] }} />
                  <span className="dw-theme-swatch-b" style={{ background: t.swatch[1] }} />
                  <span className="dw-theme-swatch-ink" style={{ background: t.swatch[2] }} />
                </span>
                <span className="dw-theme-name">{t.label}</span>
                {theme === t.id && <CheckIcon size={13} className="dw-theme-check" />}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
