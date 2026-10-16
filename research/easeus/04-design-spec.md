# Data Wizard "Revamp" — UI/UX Design Spec (EaseUS-parity port)

Port-grade spec derived from `research/easeus/` (all claims cross-verified against
extracted strings, configs, icons, class architecture). Our scope: the **recovery
experience** (no repair tools, no upsells — we are free & unlimited).

## 1. Design language (from themdark.ini token system, adapted to our 5 themes)

- **EaseUS reference metrics**: window 1240×840 default / 800×600 min · Segoe UI +
  Segoe UI Semibold 600 (light) · primary #0560fd / dark #5094ff · CTA gradient
  #0c99ff→#0560fd · success #30b373 · danger #f72c2c · radius: cards 8px, dialogs 12px,
  pills 999px.
- **Ours (evolved)**: keep the 5-theme token system (light/dark/ember/tide/blossom),
  upgrade the scale to 4px-grid spacing, 3 elevation levels, motion 150/250/400ms
  with reduced-motion support. Fonts: Inter (Latin) + system CJK fallbacks; semibold
  600 for headings/buttons, tabular-nums for all counters (the EaseUS "files found"
  odometer feel).
- Density: 36-40px result rows, 44px list headers, 56px custom chrome topbar
  (already built — keep).

## 2. Information architecture

```
Shell (topbar + step tracker + status bar)
└─ Stage machine: source → scanning → results
   ├─ SourceView (home)
   │   ├─ Header: hero line + deep-scan toggle
   │   ├─ Drive card list: internal / external / unallocated groups
   │   │    (icon by type, name, capacity bar, free/total, "Scan" CTA per row,
   │   │     system badge, TRIM badge for ssd, lock state reserved)
   │   ├─ Quick targets: Recycle Bin / specific folder / disk image import
   │   └─ Recent scans (resume chips) + refresh + device troubleshooting
   ├─ ScanningView
   │   ├─ Phase timeline: Quick scan → Advanced scan (sector-by-sector + carve)
   │   ├─ Dual progress: overall % + phase progress + sectors + rate + ETA
   │   ├─ Live found-feed (streaming rows, category chips with live counts)
   │   ├─ Controls: pause / stop (confirm) / minimize hint
   │   ├─ Tip carousel (reassurance copy from EaseUS corpus, adapted)
   │   └─ "Browse results now" (results become available during deep scan)
   └─ ResultsView (3-pane)
       ├─ Left: location tree
       │    root: source → [Deleted Files | Existing Files | Lost/RAW |
       │    Other] → type categories with counts + special groups
       ├─ Top: breadcrumb + history back/fwd + search + filter btn +
       │    view toggle (list/grid) + group-by + sort menu + selection bar
       ├─ Center: virtualized table / thumbnail grid + streaming rows
       └─ Right: inspector (details, status, recoverability, preview stub)
```

## 3. State architecture (the critical wiring layer)

Split into domain stores with single-owner state + derived selectors:

- `state/disks.ts` — inventory, auto-refresh, health/error, saved scans
- `state/scan.ts` — lifecycle (start/resume/cancel), phase machine, rate/ETA,
  event→store pumps (persistent listeners, installed once at boot — the DiskGenie lesson)
- `state/results.ts` — tree index (built from parent_path), filters (category/
  status/size/date/extension/custom), search, sort, group, selection sets,
  pagination + live append, stats cache
- `state/recovery.ts` — destination validation, session progress, outcomes
- `state/ui.ts` — toasts, dialogs, inspector, confirmations

Rules:
1. Every store mutation is the ONLY write path (no local useState mirroring domain
   state in views).
2. Event pumps translate backend events → store actions 1:1.
3. Views subscribe via selectors; derived data (tree, filtered counts, selection
   summaries) via memoized selectors to keep 100k+ row rendering at 60fps.
4. The mock backend implements the same command/event surface (already does).

## 4. Component inventory (build list)

Shell: TopBar (keep, refine), StepTracker, StatusBar, CaptionButtons, ThemePicker
Home: DriveCard, DriveList, PartitionRows (expandable), RecentScans, EmptyStates,
TroubleshootingFlyout, FolderPicker, ImageImport
Scan: PhaseTimeline, DualProgress, FoundFeed, CategoryChips, TipCarousel,
ScanControls, QuickScanCompleteBanner
Results: ResultsTree (+ special groups), Toolbar (Breadcrumb, HistoryNav, SearchBox,
FilterButton, ViewToggle, GroupMenu, SortMenu), FilterDrawer (TypeTree, SizeRanges,
DateRanges, ExtensionFilter, JunkToggles), ResultsTable, ResultsGrid, SelectionBar,
RowRenderer, TypeIcon (per-extension 24px), RecoverabilityBadge
Recovery: DestinationDialog (Local/Cloud tabs, validation), RecoveryProgress
(stream, counters, failures), RecoverySummary (report, open location)
Common: Button (primary gradient/ghost), Checkbox, Menu, Flyout, Tooltip, Toast,
ConfirmDialog, EmptyState, OdometerCounter (animated number), ProgressBar,
CapacityBar, StatusPill, TagPill

## 5. Motion & feedback (the "satisfying" layer)

- Counters animate (odometer-style lerp, tabular-nums)
- Progress: gradient fill + shimmer at active edge; phase transitions crossfade
- Rows stream in with stagger (max 12ms/frame batching to stay jank-free)
- Tree/category counts pop (scale 1.0→1.08→1.0) on update
- Dialogs: scale 0.96→1.0 + backdrop blur; confirm destructive with danger accent
- Button press: scale 0.98; hover: 2% luminance lift
- Scanning view: the animated sector-scan illustration (CSS, per theme)
- All motion disabled under prefers-reduced-motion

## 6. Copy corpus (adapted from extracted EaseUS strings)

Reassurance at the right moments (see 03-user-flows.md §Global UX). We keep the
severity-appropriate warnings (source-drive overwrite prevention, stop-scan
confirmations) — they are safety UX, not upsell.
