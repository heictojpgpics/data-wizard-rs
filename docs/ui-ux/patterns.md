# UI/UX Patterns (information architecture only)

> **This document describes user flows and information architecture —
> NOT pixel layouts, NOT assets, NOT strings.** Our UI gets its own
> visual identity. We document the reference product's UX patterns
> here only to understand what users expect from a data-recovery
> wizard.

## Reference product's user flow (observable from running the free
version)

1. **Launch**: splash screen → main window.
2. **Select source**: a list of disks + partitions + "Browse for file
   image" option. Each device shows: display name, size, type
   (fixed/removable), and a small icon.
3. **Pick scan mode**: "Quick" (default) or "Deep" (with a tooltip
   explaining the trade-off).
4. **Scan progress**: a window with a progress bar, current phase
   ("Probing" → "Quick scan" → "Deep scan" → "Finalizing"), a
   live-updating count of "files found", and a Cancel button.
5. **Results browser**: a tree view on the left (folder hierarchy) +
   a list view on the right (files in the selected folder). Each file
   shows: name, size, modification date, status (intact/deleted/
   carved), and a checkbox for selection. A filter bar at the top
   lets the user filter by extension, size, date, or status.
6. **Preview**: double-clicking a file opens a preview pane (image
   thumbnail, PDF view, text view, etc.).
7. **Recover**: select files → click "Recover" → pick a destination
   directory → progress dialog → success/failure summary.
8. **Resume**: scans can be saved to disk and resumed later.

## What's good about this flow (and we'll mirror)

- **Quick vs Deep upfront.** Users don't want to wait an hour for a
  deep scan if a quick scan would have found their file. The choice
  should be presented before the scan starts.
- **Live results.** Showing files as they're found (not waiting for
  the scan to finish) gives the user confidence the scan is working.
- **Tree + list dual-pane.** Familiar from Windows Explorer. Users
  don't have to learn a new navigation paradigm.
- **Filter bar.** Essential when a scan returns millions of files.
- **Preview before recover.** Users want to verify a file is intact
  before committing to recovery.
- **Save/resume scan.** Scans can take hours; users need to be able
  to close the app and resume later.

## What's bad (and we'll fix)

- **The reference product nags the user constantly**: upgrade prompts,
  "rate us" dialogs, in-app messages. We will NOT do this.
- **The reference product's filter bar is modal**: you can't filter
  while a scan is running. We will allow filtering during the scan.
- **The reference product doesn't show scan ETA**: just a progress
  bar. We will compute and show an ETA based on throughput.
- **The reference product's preview is limited**: it can't preview
  many file types. We will use the OS's built-in previewers via
  `tauri-plugin-opener` for types we don't natively support.
- **The reference product's recovery dialog doesn't verify**: it
  writes files and reports success even if the destination ran out
  of space mid-write. We will verify each file's checksum after write.

## Our UI flow (v0.1)

```
┌──────────────────────────────────────────────────────────┐
│  [Header: Data Wizard logo + version + GitHub link]      │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  Select a source                                         │
│  ┌────────────────────────────────────────────────┐      │
│  │ Samsung SSD 980 1TB                            │      │
│  │   └─ Partition 1: C: (NTFS, 500 GB) [System]  │      │
│  │   └─ Partition 2: D: (NTFS, 500 GB)           │      │
│  │ USB Flash Drive 32 GB                          │      │
│  │   └─ Partition 1: E: (FAT32, 32 GB)           │      │
│  │ Browse for file image...                       │      │
│  └────────────────────────────────────────────────┘      │
│                                                          │
│  [Quick scan]  [Deep scan]                              │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

```
┌──────────────────────────────────────────────────────────┐
│  [Header]                                                │
├──────────────────────────────────────────────────────────┤
│  Scanning Samsung SSD 980 - Partition 1 (C:)             │
│  Phase: Deep scan                                        │
│  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░  67%             │
│  12,345 of 18,403 clusters · ETA 2m 15s                  │
│  1,234 files found so far                                │
│                                                          │
│  [Cancel scan]                                           │
└──────────────────────────────────────────────────────────┘
```

```
┌──────────────────────────────────────────────────────────┐
│  [Header]                                                │
├──────────────────────────────────────────────────────────┤
│  ← New scan    Results: 12,345 files                     │
│  [Filter: All types ▾] [Size: Any ▾] [Date: Any ▾]      │
├─────────────────────────┬────────────────────────────────┤
│  Folders                │  Files                         │
│  ▾ C:\                  │  ☑ document.docx  12 KB        │
│    ▾ Users              │  ☑ photo.jpg      2.1 MB       │
│      ▾ alice            │  ☑ report.pdf     340 KB       │
│        ▾ Documents      │  ☐ temp.txt        0 B  [del]  │
│        ▾ Pictures       │  ☐ cache.bin     1.2 MB  [del] │
│    ▾ Program Files      │                                │
│  ▾ [Lost files]         │                                │
│    ▾ JPEG                │                                │
│    ▾ PDF                 │                                │
├─────────────────────────┴────────────────────────────────┤
│  Selected: 3 files (2.5 MB)      [Recover to...]         │
└──────────────────────────────────────────────────────────┘
```

## Our design principles

1. **Calm UI.** No modals, no nags, no upsells. The user is here to
   recover files, not to be sold something.
2. **Live everything.** Results stream in live. Filters apply live.
   Progress is live. ETA is live.
3. **Type-safe end-to-end.** Every UI element is backed by a typed
   Rust struct via `tauri-specta`. No stringly-typed APIs.
4. **Keyboard-first.** Every action has a keyboard shortcut. Power
   users shouldn't need the mouse.
5. **Dark by default.** Recovery is often done in low-light
   conditions (server rooms, disaster-recovery scenarios). Dark
   theme is the default; light theme is one toggle away.
6. **Accessible.** WCAG AA. Screen-reader friendly. High-contrast
   mode.
7. **Localizable.** i18next with JSON locale files. We ship en-US
   by default; other locales are community-contributed.
