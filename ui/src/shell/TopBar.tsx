/**
 * Top bar (56px): brand mark + name → wizard step tracker (Source ·
 * Scan · Results with a sliding ink pill) → spacer/drag region →
 * read-only trust chip → theme quick toggle + picker → GitHub link →
 * Windows caption buttons. The whole bar doubles as the window drag
 * region (Windows 11 app convention — one chrome row instead of a
 * separate title bar).
 */
import { useEffect } from "react";
import { MoonIcon, ShieldCheckIcon, SunIcon } from "../lib/Icon";
import { BrandGlyph } from "../lib/Icon";
import { useTheme } from "../theme/useTheme";
import { useAppStore, type Stage } from "../state/app";
import { useScanStore } from "../state/scan";
import { CaptionButtons, useWindowControls } from "./CaptionButtons";
import { ThemePicker } from "./ThemePicker";

const STEPS: { id: Stage; label: string }[] = [
  { id: "source", label: "Source" },
  { id: "scanning", label: "Scan" },
  { id: "results", label: "Results" },
];

const STEP_INDEX: Record<Stage, number> = { source: 0, scanning: 1, results: 2 };

export function TopBar() {
  const stage = useAppStore((s) => s.stage);
  const scanPhase = useScanStore((s) => s.progress?.phase ?? null);
  const { isDark, toggle } = useTheme();
  const windowApi = useWindowControls();

  // Double-click on empty bar toggles maximize (Windows caption
  // convention). Interactive controls opt out via the closest() guard.
  const onDoubleClick = (e: React.MouseEvent) => {
    if (!windowApi) return;
    const t = e.target as HTMLElement;
    if (t.closest("button, input, a, [role='menu'], .dw-theme-picker")) return;
    void windowApi.toggleMaximize();
  };

  // A step is "done" when the flow moved past it; the scan step is
  // done only when the engine reported a terminal phase.
  const scanDone =
    scanPhase === "Done" || scanPhase === "Cancelled" || scanPhase === "Failed";
  const stepDone = (i: number): boolean => {
    const cur = STEP_INDEX[stage];
    if (i < cur) return i === 1 ? scanDone : true;
    return false;
  };

  useEffect(() => {
    // Window show: reveal after first paint (created hidden in
    // tauri.conf.json so there is no white flash before the theme
    // CSS lands). Plain-browser mock: nothing to do.
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().show())
      .catch(() => undefined);
  }, []);

  return (
    <header className="dw-topbar" data-tauri-drag-region onDoubleClick={onDoubleClick}>
      <div className="dw-brand" data-tauri-drag-region>
        <span className="dw-brand-mark">
          <BrandGlyph size={22} />
        </span>
        <span className="dw-brand-name">Data Wizard</span>
      </div>

      <nav className="dw-steps" aria-label="Wizard steps">
        {STEPS.map((s, i) => {
          const active = stage === s.id;
          const done = stepDone(i);
          return (
            <div key={s.id} className={`dw-step${active ? " dw-active" : ""}${done ? " dw-done" : ""}`}>
              <span className="dw-step-dot" aria-hidden />
              <span className="dw-step-label">{s.label}</span>
              {i < STEPS.length - 1 && <span className="dw-step-line" aria-hidden />}
            </div>
          );
        })}
      </nav>

      <div className="dw-topbar-spacer" data-tauri-drag-region />

      <div className="dw-readonly-chip" title="Scanning only reads from the source. Recovery writes go to the destination you choose.">
        <ShieldCheckIcon size={13} />
        Read-only scan
      </div>

      <button
        className="dw-topbar-iconbtn"
        aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"}
        title={isDark ? "Light theme" : "Dark theme"}
        onClick={toggle}
      >
        {isDark ? <SunIcon size={16} /> : <MoonIcon size={16} />}
      </button>

      <ThemePicker />

      <a
        className="dw-topbar-link"
        href="https://github.com/heictojpgpics/data-wizard-rs"
        target="_blank"
        rel="noreferrer"
        title="Source on GitHub"
      >
        GitHub
      </a>

      <CaptionButtons />
    </header>
  );
}
