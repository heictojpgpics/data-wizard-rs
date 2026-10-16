# Data Wizard Design System

The visual language of the app, distilled. This documents the token
system, the theme architecture, the motion family, and the layout
grammar every view follows. The single source of truth for values is
[`ui/src/theme/tokens.css`](../../ui/src/theme/tokens.css); this page
explains the intent behind them.

## Design concept: the calm recovery studio

A data-recovery session is an emergency. The UI's job is to feel like
a calm professional walking a stressed user through a fixed procedure:
**restrained surfaces, one ink accent, information that arrives in the
order decisions are made**. Three principles drive every screen:

1. **Calm authority** — generous whitespace, muted neutrals, a single
   teal accent reserved for primary actions and "you are here" states.
   Nothing blinks unless the engine is working.
2. **Progressive disclosure** — the wizard (Source → Scan → Results)
   carries the flow; advanced detail (extents, confidence, collision
   policies) lives in the inspector and dialogs, never the main path.
3. **Visible safety** — read-only scanning is stated where scanning
   starts (topbar chip, scan card, sidebar notes), and recovery always
   names its destination before it writes.

## Theme architecture (two layers)

Five palettes; the same two-layer attribute system as DiskGenie:

| Theme | Family | Surfaces | Ink |
|---|---|---|---|
| `light` (default) | light | near-white `#f6f8f7` + white cards | recovery teal `#0d9488` |
| `dark` | dark | graphite `#171b1a` | brightened teal `#2dd4bf` |
| `ember` | dark | warm espresso `#201a15` | honey gold `#f2b04e` |
| `tide` | dark | deep marine `#101722` | lagoon cyan `#3fc5f2` |
| `blossom` | light | blush cream `#fbf1ee` + white cards | rose `#d2557a` |

- `data-theme="<id>"` selects the palette block in tokens.css.
- `data-scheme="light|dark"` selects the FAMILY layer (control
  affordances, vivid category colors, elevation depth, color-scheme).
- A pre-mount inline script in `index.html` applies the persisted
  choice (localStorage, key `datawizard.theme`) before React mounts —
  no flash. The background-color ladder in the same script covers the
  first paint.
- Switching themes crossfades via the View Transitions API (WebView2;
  instant under `prefers-reduced-motion`) and syncs the native window
  theme to the family.

### Why teal

Teal is the color of restoration (first aid, water, stability). It
keeps Data Wizard's identity distinct from the coral DiskGenie while
carrying the right emotional register: clinical, safe, precise.

## Tokens

- **Surfaces**: `--background`, `--panel`, `--card`, `--border`,
  `--divider`, `--chip`, `--track`, `--overlay`.
- **Text**: `--text`, `--text-secondary`, `--text-tertiary`
  (+ `--on-pastel` pair for text over category pastels).
- **Brand**: `--ink`, `--ink-strong`, `--on-ink`, `--ink-soft`,
  `--ink-grad` (CTA gradient), `--ink-glow` / `--ink-glow-strong`.
  High-luminance inks (ember gold, tide cyan) flip `--on-ink` to a
  dark tone — the whole button vocabulary follows automatically.
- **Category colors** (data, theme-independent pastels on light /
  vivid family on dark): photos sky, videos rose, audio violet,
  documents amber, archives orange, email green, code teal, other
  slate.
- **Recovery status colors**: `--st-intact` green, `--st-validated`
  teal, `--st-carved` blue, `--st-partial` amber, `--st-overwritten`
  red, `--st-pending` gray.
- **Shape**: radii 6/8/10/12/14; press scale 0.985.
- **Elevation**: `--shadow-1/2/3` + `--shadow-pop` (dialogs, the
  selection bar) with a crisp 1px contact edge.
- **Spacing**: 4px base with 6/10/14 half-steps for dense rows.

## Motion

One rhythm family, durations on a ladder anchored at the 160ms fade:
micro 120 (chips, hovers) · fast 160 (entrances, scrim) · base 240
(layout settles) · slow 360 (large surfaces). Easings: ease-out
`cubic-bezier(0.2, 0.8, 0.3, 1)`, overshoot for pop-ins. All view
transitions are pure CSS keyframes (`dw-settle-in` and friends) —
compositor-driven, no inline-style gaps (the lesson recorded across
DiskGenie's session worklogs). `prefers-reduced-motion` collapses
every animation to instant.

Indeterminate loaders share one rhythm: spinner 0.92s, shimmer 1.38s,
progress sheen 1.15s.

## Layout grammar

- **Top bar (56px)** — the window drag region: brand mark, wizard
  step tracker (sliding ink pill), read-only trust chip, theme
  controls, GitHub, Windows caption buttons.
- **Body** — contextual sidebar (304px) | main | inspector (360px on
  results). The sidebar's content follows the stage: saved scans +
  safety education on source, live scan context while scanning, the
  category rail + scan actions on results. A device tree was
  deliberately rejected — it fights the linear wizard.
- **Status strip (30px)** — stage, counts, selection size, version.
- **Density ladder** — ≤1440px compacts the columns; ≤1180px floats
  the inspector as an overlay.

## Control vocabulary

`dw-btn-ink` (gradient + glow, the one primary action per view) ·
`dw-btn-outline` (secondary; `dw-danger` variant) · `dw-btn-ghost`
(tertiary / icon chrome) · `dw-chip` (filters, badges) · `dw-input` /
`dw-select` / `dw-check` / `dw-track` (progress) · skeletons +
spinner for loading. Every interactive element carries the 2px
`--focus-ring` on `:focus-visible`.

## Copy voice

Plain, honest, loss-aware: statuses say what they mean ("Carved,
verified"), damaged files are never hidden, and safety claims are
specific ("temp + fsync + atomic publish") rather than generic.

## The mock layer (dev/browser only)

`ui/src/mock/` implements the full Tauri command surface against a
synthetic recovery scenario (4 disks, ~120 generated deleted files,
phased scan ticker, recovery stream). It is installed by `main.tsx`
only when `!isTauri() && import.meta.env.DEV` — the dynamic import
keeps it out of production bundles. Browse with `npm --prefix ui run
dev` and drive the whole UI, including the screenshot tour
(`http://localhost:5173/?tour=1`), in a plain browser.

## CI screenshot tours

`ui-screenshots.yml` builds the real app, stages a VHD with a deleted
file population, launches with `DATAWIZARD_TOUR=1` +
`DATAWIZARD_SCAN=\\.\T:`, and captures one named frame per tour step
through the title-sync channel (`Data Wizard · tNN-name`, stamped by
the TourDriver). The same tour runs in the browser mock via `?tour=1`
for agent-driven visual validation.

## The revamp layer (EaseUS-parity, 2026-10)

The full spec derivation lives in `research/easeus/` (reverse-engineered
from the reference product). The revamp keeps the calm-studio language
and adds the parity mechanics:

**Domain stores** (single-owner state, the DiskGenie lesson): `devices`
(inventory + saved scans + focus re-warm), `scan` (phase machine incl.
the quick→deep interstitial, pause, ETA, the 350ms live-results pump),
`results` (full in-memory set; `byId`/`childFiles`/`childDirs`/`subtree`
indexes; category/special-group/status/date counters — all copy-on-write
so zustand selectors stay live; the filter/sort/group/browse engine),
`recovery` (destination validation + per-file stream), `ui` (toasts,
confirms). `app` is deliberately thin: the stage machine + scan identity.

**The results grammar**: left tree (source root, Deleted/Existing/Lost/
Recycle special groups, 8-type rail, folders-here), toolbar (history
back/forward, breadcrumb, debounced search, filter badge, list/grid,
group-by, sort), virtualized table + thumbnail grid, filter drawer
(types with counts, EaseUS size/date boundaries, extension query,
clutter toggles), selection bar. Recoverability is a 4-step verdict
(Excellent/Good/Poor/Overwritten) derived from status + confidence.

**Motion**: odometer digits on rail transitions, count-pop on growing
counters, shimmer on active progress, staggered row-in on the live feed,
pop-in dialogs — all collapsed under `prefers-reduced-motion`.

**Pause**: `PausableSource` (dw-disk) gates both the sync `ReadAt` view
and the async path on a shared `AtomicBool`; `set_scan_paused` flips it.
Zero engine changes — the whole pipeline stalls in place.

**Agent channels**: `?tour=1` browser tours (TourDriver steps: home ×3,
themes ×5, mid-scan, results ×8, recovery ×2 — title-stamped `tNN-*`);
`window.__DW_STORES__` (DEV/browser mock only) exposes the live stores
for scripted validation; `window.__DW_MOCK__.finishScan()` fast-forwards
the synthetic scan.
