/**
 * Theme registry — the Data Wizard two-layer attribute system.
 *
 *   data-theme="<id>"         the concrete palette (tokens.css block).
 *   data-scheme="light|dark"  the FAMILY the palette belongs to.
 *
 * `light` and `dark` are the canonical pair (the topbar quick toggle
 * flips between exactly those two; custom palettes live in the picker).
 *
 * Palettes:
 *   light    near-white surfaces + recovery teal (the shipped default)
 *   dark     neutral graphite + teal
 *   ember    warm espresso surfaces + honey-gold ink
 *   tide     deep marine surfaces + lagoon-cyan ink
 *   blossom  blush cream surfaces + rose ink
 */

/** The persisted theme ids, in picker order. */
export const THEME_IDS = ["light", "dark", "ember", "tide", "blossom"] as const;

export type ThemeId = (typeof THEME_IDS)[number];

export type ThemeFamily = "light" | "dark";

/** Display metadata — the picker's labels + swatch colors. */
export interface ThemeMeta {
  id: ThemeId;
  label: string;
  family: ThemeFamily;
  /** Three swatch colors for the picker preview: surface, surface-alt, ink. */
  swatch: [string, string, string];
}

export const THEMES: Record<ThemeId, ThemeMeta> = {
  light: { id: "light", label: "Light", family: "light", swatch: ["#f6f8f7", "#eef1f0", "#0d9488"] },
  dark: { id: "dark", label: "Dark", family: "dark", swatch: ["#171b1a", "#212725", "#2dd4bf"] },
  ember: { id: "ember", label: "Ember", family: "dark", swatch: ["#201a15", "#2e2620", "#f2b04e"] },
  tide: { id: "tide", label: "Tide", family: "dark", swatch: ["#101722", "#1c2534", "#3fc5f2"] },
  blossom: { id: "blossom", label: "Blossom", family: "light", swatch: ["#fbf1ee", "#ffffff", "#d2557a"] },
};

/** The picker's ordered list (THEME_IDS guaranteed present). */
export const THEME_LIST: ThemeMeta[] = THEME_IDS.map((id) => THEMES[id]);

/** Family lookup for the scheme attribute + dark-family CSS branches. */
export function familyOf(id: ThemeId): ThemeFamily {
  return THEMES[id].family;
}

/** The pre-mount/apply DOM writes for one theme — BOTH attributes,
 * always together (every consumer of data-scheme depends on this
 * pairing, including the mock tour driver and canvas branches). */
export function applyThemeAttributes(id: ThemeId): void {
  const root = document.documentElement;
  root.setAttribute("data-theme", id);
  root.setAttribute("data-scheme", familyOf(id));
}

/** The native window theme the Tauri side should match (families map
 * onto the two values setTheme accepts; custom themes follow their
 * family so native menus/tooltips stay coherent). */
export function nativeThemeFor(id: ThemeId): "light" | "dark" {
  return familyOf(id);
}

const STORAGE_KEY = "datawizard.theme";

/** Validate a stored/loaded value — anything unknown falls back to
 * the light default (the pre-mount script and useTheme share this). */
export function validateThemeId(value: string | null | undefined): ThemeId {
  return value != null && (THEME_IDS as readonly string[]).includes(value)
    ? (value as ThemeId)
    : "light";
}

/** Read the persisted choice (best-effort; hardened WebView profiles
 * can throw on localStorage access). */
export function readSavedTheme(): ThemeId {
  try {
    return validateThemeId(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return "light";
  }
}

/** Persist a choice (same best-effort contract as readSavedTheme). */
export function persistTheme(id: ThemeId): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, id);
  } catch {
    // localStorage can throw in hardened WebView profiles; the theme
    // still applies for this session, it just will not persist.
  }
}

/** Read the theme the pre-mount script already applied (fallback:
 * light — matches the default the script guarantees). */
export function currentDomTheme(): ThemeId {
  const dom = document.documentElement.getAttribute("data-theme");
  return validateThemeId(dom);
}
