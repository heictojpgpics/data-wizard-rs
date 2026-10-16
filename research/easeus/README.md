# EaseUS Data Recovery Wizard — UI/UX Reverse Engineering

**Session**: 2026-10-16 · **Branch**: `revamp` · **Analyst**: agentic RE pass
**Subject**: EaseUS Data Recovery Wizard 21.0 (free, `RE-DRW-FREE-WIN`, Qt/Widgets x64)
**Goal**: extract the exact UI/UX architecture (flows, screens, controls, semantics, visual system) to parity-port the *recovery experience* into Data Wizard (excluding their ancillary repair tools).

## Source material

Two zip parts downloaded and merged into one folder (1,470 files, 674 MB):
- `EaseUS-Data-Recovery-Wizard-Part-1.zip` + `Part-2.zip` → full installed product tree.

## Toolchain used (all no-sudo local installs)

| Tool | Role | Status |
|---|---|---|
| Ghidra 11.3.2 headless + JDK 21.0.4 | disassembly/decompile | ready |
| radare2 5.9.8 (rabin2) | PE recon | used |
| FLOSS (pip) | string deobfuscation | used |
| GNU binutils (objdump/strings/nm) | PE + string recon | used |
| polib (py) | gettext .mo parsing | used |
| custom QM parser (this repo, `scripts/qm_scan.py`) | Qt .qm translation extraction | used — 1,556 UI strings |
| custom RCC extractor (`scripts/rcc_extract.py`) | Qt .rcc resource extraction | used — 7,379 assets (light+dark) |

(DIE/Wine unavailable without sudo; substituted with rabin2+Ghidra+custom parsers — no capability lost for this target.)

## Verified findings so far

### 1. App architecture (from folder)
- `DRWUI.exe` — Qt Widgets UI process (the wizard). `DRW.exe` — scan/recovery engine.
  `DRWService.exe` — service. `EUinApp.exe` — in-app store/upsell. `DRWReport.exe` — reports.
- Qt platform plugins shipped: `qwindows.dll`, `qminimal.dll`, `qoffscreen.dll` (headless usable!),
  style `qwindowsvistastyle.dll`, imageformats, iconengines (QtSvg).
- Light/dark theme via **two RCC bundles**: `Resource/drw_light.rcc` (30.7 MB) + `ResourceDark/drw_dark.rcc` (32.3 MB).

### 2. Internationalization = UI text inventory
- `language/` holds 22 `.qm` files (Arabic…Turkish). English.qm parsed (custom parser; Qt QM
  format, UTF-16BE names, 4-byte qt_hash) → **1,556 source strings** (`raw/English_all_strings.txt`).
- Contexts are almost all `QObject` (bare `QObject::tr`), plus `RecoverProgressDialog`,
  `ChooseDestinationPathDialog(Class)`, `ChooseDestinationPathItem(Class)` — the few
  *named-widget* strings give the exact dialogs for the recovery flow.
- Strings include full wording of: source-select screen, quick/advanced scan states, filter panel,
  category rail, previews, recover dialogs, warnings, NAS/Linux/RAID/BitLocker/Cloud flows.

### 3. Theme system (`config/themdark.ini` — 118 tokens × 2 themes)
Light: primary `#0560fd`, bg gradient `#eff5ff→#deeaff`, surface `#ffffff`, text `#1a1a1a`,
muted `#616777`, CTA gradient `#0c99ff→#0560fd`, success `#30b373`, danger `#f72c2c`, warn `#ffc10e`.
Dark: primary `#5094ff`, bg gradient `#1b1d1f→#18191b`, surface `#202428`, text `#f1f5ff`,
muted `#b0b5c0`, CTA gradient `#059afd→#0ccdff`, success `#349a37`, danger `#ff5a5a`, warn `#E29100`.

### 4. Typography & window (`config/config.ini`)
- Font: **Segoe UI / Segoe UI Semibold (600)**; locale-aware fallbacks (Meiryo UI, Malgun Gothic, Microsoft YaHei).
- Window: **1240×840 default, 800×600 minimum**, high-DPI aware, remembers maximized state.

### 5. Results taxonomy (from strings + configs)
- Type view: Picture/Video/Document/Audio/Archive/Email/Other with per-type extension priority
  (`PriorityDisplayType.xml`), sizes <512KB / 1MB~512KB / 512MB~1GB / >1GB, dates (Today/
  Yesterday/Last 7 days/Last 1 month), special groups: **Deleted Files, Existing Files, Lost Name
  (RAW recovered), Path Lost, Tags Extracted (EXIF/metadata), Directory Intact, Unimportant
  Files, Duplicates, Recycle Bin, SD/Camera/UAV/tachograph sources, BitLocker partitions**.
- Group by: Type / Date Modified / Tag · View: Path (tree) / Type · Sort: Name/Size/Date (asc/desc).
- RAW carve filter list (`RawConfig.ini`): ~60 extensions (HEIC, WEBP, INSV, …).
- Junk filter (`FilterFolder.ini`): $Windows.~BT, Config.Mi, System Volume Information,
  desktop.ini, localized "New Folder", etc. + "SummaryFileType" merge map (jpg+jpeg, tif+tiff…).

### 6. Icon & illustration system (extracted from both .rcc files)
`assets/` + full manifests (`raw/*_rcc_manifest.json`): DriveTypes (every state × lock × lost),
Home nav (PC/SD/NAS/RAID/Cloud/Bitlocker/Crashed/PE × nor/sel/dis), FileTypes 24/48 SVG per
extension + special folders (File Path Lost, Tags Extracted, Unimportant) + browser-cache icons,
ScanProgress animation sequences (Scanmove purple, ScanComplete, ScanStop, scanloading),
Preview loading sequences per media type, Evaluate smileys (happy/general/anger), Titlebar,
LeftTree, Path breadcrumb icons, NAS/PE effect sequences, WndShadow, ScollBar…

### 7. Scan pipeline semantics (from strings)
- Two-phase: **Quick Scan** (deleted files + lost partitions, minutes) → **Advanced/Deep Scan**
  (sector-by-sector, runs *alongside* streaming results: "Advanced Scanning %1, Files Found:").
- Live counters: scanned sectors, files found, elapsed, remaining, estimated.
- Auto-Recover option ("recover automatically after advanced scan completes").
- Bad-sector skip behavior, disk-image scan recommendation for failing disks.
- Session save/load (`*.rsf` — "Recovery State File"), resume last scan.

## Next (in progress)
- Ghidra pass over DRWUI.exe/DRW.exe for flow/state machine + IPC semantics.
- PartitionRecovery .mo corpus for engine-side terminology.
- Full user-flow map + design spec for the Data Wizard port.
