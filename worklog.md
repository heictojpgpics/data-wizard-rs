# Worklog — Data Recovery Wizard (Rust + Tauri 2) Reverse-Engineering & Build

This is the single shared worklog for the project. All agents append below using the standard template.

---
Task ID: 1
Agent: main
Task: Initialize workspace, download EaseUS installers, extract merged payload, set up RE toolchain

Work Log:
- Created directory structure under /home/z/my-project/ (scripts/, download/, analysis/{raw,extracted,reports}/)
- Downloaded all 4 EaseUS artifacts via curl:
  - EaseUS-Part-1.zip (183,446,644 bytes)
  - EaseUS-Part-2.zip (171,613,300 bytes)
  - easeus_setup.dmg (4,808,563 bytes) — zlib-compressed Apple DMG
  - easeus_setup.exe (2,837,784 bytes) — NSIS installer (PE32 i386 GUI)
- Verified both zips are independent (not split) — extracted both into /home/z/my-project/analysis/extracted/merged/
- Merged payload: 1470 files, 674 MB, 168 top-level entries
- Installed Python RE libraries in venv /home/z/.venv: pefile, capstone, lief (py7zr failed — will retry)
- Began mapping top-level architecture — see findings below

Stage Summary — Top-level architecture (observable file layout, NOT decompiled code):

Process model (4 executables):
  - DRW.exe          — main orchestrator
  - DRWUI.exe        — UI process (Qt5-based)
  - DRWService.exe   — background service (likely elevated, holds raw disk handles)
  - DRWReport.exe    — scan result report generator
  - EuRepairHost.exe — host process for file-repair subprocesses
  - EUinApp.exe      — in-app messaging / upgrade UI
  - InfoForSetup.exe — setup helper

Filesystem parsers (modular DLLs, one per FS — clean design we will mirror):
  - NTFSSearchDLL.dll     — NTFS
  - FATSearchDLL.dll      — FAT12/16/32
  - exFATSearchDLL.dll    — exFAT
  - EXT4SearchDLL.dll     — ext2/3/4
  - HFSPlusSearchDLL.dll  — HFS+ / APFS-adjacent
  - BtrfsSearchDLL.dll    — Btrfs
  - ReFSSearchDLL.dll     — ReFS
  - ZfsSearchDLL.dll      — ZFS
  - RAWSearchDLL.dll      — raw / unknown filesystem carving fallback

Auxiliary modules:
  - PE_Tools.dll + PEtools.xml   — PE-portable-executable reconstruction
  - PartitionRecovery/           — partition table recovery (likely MBR/GPT rebuild)
  - diskhealth/                  — SMART / disk health
  - EUFileRepair/                — file-format repair (Office, Adobe, Archive, HWP, Image, Video, Raw)
  - SaveScan/                    — saved scan-state format
  - Dict/                        — dictionary (likely file-type signature dictionary)
  - Resource/, ResourceDark/     — UI resources (light/dark themes) — we will NOT copy these

Cloud & telemetry (we will REPLACE with own / opt-in only):
  - AliyunWrap.dll, AliyunConfig.ini, AliyunWrapExe.exe  — Aliyun OSS integration
  - cloud_session.dll, send_dmp_with_aliyun.exe          — crash dump upload
  - firebasefetch.exe, firebaseonlineconfig.dll          — Firebase remote config
  - google-services.json                                 — Firebase config (project metadata)
  - remote_config_data/, dlconfig.xml                    — remote feature flags

Signature databases (these we WILL rebuild from public sources, NOT extract from their files):
  - raw_signs.json    — raw file signature DB (carving)
  - FileFilter.xml    — file type filter config
  - PEtools.xml       — PE reconstruction rules

UI stack (observable from DLL presence):
  - Qt5Core, Qt5Gui, Qt5Widgets, Qt5Network, Qt5Svg, Qt5WinExtras — Qt 5.x
  - VC++ runtime: msvcr90, msvcp90, msvcp120, msvcr120, msvcp140, vcruntime140, concrt140
  - OpenSSL 1.0.x (libeay32/ssleay32) + 1.1 (libcrypto-1_1/libssl-1_1) + 3.x (libcrypto-3/libssl-3)
  - libssh2, libzmq-mt-4_3_5 (ZeroMQ — IPC between DRW.exe and DRWService.exe likely)
  - libx265, heif, de265, pdfium, zlib1, liblzma

WinPE rescue environment (BUILDPE/):
  - Custom kernel drivers (signed): EUDCPEPM.sys, EUEDKEPM.sys, ebrntdrv.sys, epmdkdrv.sys
  - These are the low-level disk access drivers used in WinPE boot mode
  - We will implement equivalent functionality via Windows DeviceIoControl + our own kernel driver later

Licensing (we DOCUMENT publicly visible gates, NEVER bypass):
  - License/ directory + recoveryFree.ini — free-tier feature flags
  - We will catalog what's paywalled at a feature level only, then implement equivalent features in our free OSS version

Artifacts produced so far:
- /home/z/my-project/download/ — 4 downloaded source files
- /home/z/my-project/analysis/extracted/merged/ — 1470-file merged payload
- /home/z/my-project/worklog.md — this file

Next steps (Task 2+):
- Build machine-readable file inventory (file type, size, PE metadata, imports/exports)
- Catalog every DLL's exported symbols (these are public API surface)
- Map dependency graph (which DLL imports which)
- Extract FileFilter.xml, raw_signs.json, PEtools.xml structure (formats only — we rebuild data from public sources)
- Decompose NSIS installer (easeus_setup.exe) with 7zip once available
- Decompose DMG (zlib stream) once 7zip available
- Cross-validate findings with Ghidra headless on key DLLs (NTFSSearchDLL, FATSearchDLL, DRW.exe)
- Document algorithms conceptually (FS parsing, signature scanning, file carving, fragment reassembly)
- Initialize Rust + Tauri 2 + React/TS workspace
- Initialize CI/verification repo for Windows GitHub Actions

---
Task ID: 2
Agent: main
Task: Build Rust workspace scaffolding, write RE documentation, create Tauri 2 + React UI shell, push initial commit

Work Log:
- Analyzed extracted payload: 1470 files, 535 PE binaries catalogued in inventory.json + inventory.md
- Extracted public API surface of 28 core modules -> core_api_surface.md
  - Key finding: each FS parser DLL has 2 exports (Quick + Full searcher factory)
  - Key finding: RAWSearchDLL has 6 exports including "analyser" factories for file-type validation
  - Key finding: DRW.exe and DRWReport.exe lack ASLR (security weakness we won't replicate)
  - Key finding: no CFG anywhere in product binaries
- Documented FileFilter.xml schema (8 categories, ~1469 extensions) and raw_signs.json schema
  ({ext: {header: {signs}, tail: {signs}, min, max}}) - FORMAT ONLY, we rebuild data from public sources
- Designed Rust workspace with 13 crates:
  - dw-core (orchestrator), dw-disk (raw I/O), dw-ipc (types)
  - dw-ntfs, dw-fat, dw-exfat, dw-ext, dw-hfs, dw-btrfs, dw-refs, dw-zfs (FS parsers, one per FS)
  - dw-signatures (rebuilt from public sources), dw-carver (header/footer + bifragment + structured)
- All crates compile cleanly with `cargo check --workspace` (only 2 minor warnings)
- Set up Tauri 2 shell (src-tauri/) with 6 commands: list_disks, start_scan, cancel_scan, get_scan_progress, list_results, recover_files
- Set up React 19 + TypeScript + Vite 7 + Tailwind 4 UI (ui/) with select/scanning/results views
- Wrote CI workflow (.github/workflows/ci.yml): fmt, clippy, test-linux, build-windows, test-windows
- Wrote release workflow (.github/workflows/release.yml): MSI + NSIS + portable ZIP
- Wrote documentation artifacts in docs/:
  - INDEX.md (cross-link map)
  - architecture/overview.md, process-model.md, fs-parser-pattern.md, licensing-boundary.md
  - algorithms/scan-phases.md, ntfs-recovery.md, carving.md
  - security/findings.md (reference product posture), our-posture.md (our posture)
  - tech-stack/reference.md, ours.md
  - glossary.md
- Wrote SECURITY.md, CONTRIBUTING.md, LICENSE-MIT, LICENSE-APACHE
- Created GitHub repos:
  - data-wizard-rs (main project, public) - id 1420824092
  - data-wizard-ci (verification, public) - id 1420824678

Stage Summary:
- Workspace compiles: 13 crates, `cargo check --workspace` passes
- 8 RE documentation artifacts written, cross-linked via docs/INDEX.md
- Tauri 2 + React 19 UI shell functional (placeholder views, real wiring next)
- CI workflows configured for both Linux (lint+test) and Windows (build+test)
- Ready for first commit + push to GitHub

Next steps (Task 3+):
- Push initial commit to data-wizard-rs
- Set up data-wizard-ci repo with Windows verification workflows
- Implement NTFS probe + MFT walker in dw-ntfs (first real FS parser)
- Implement disk enumeration (Windows physical drives + volumes) in dw-tauri list_disks command
- Wire up scan progress events (Tauri events from Rust core)
- Build first end-to-end smoke test on Windows CI
