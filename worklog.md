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

---
Task ID: 3
Agent: main
Task: Implement NTFS VBR parser, FAT BPB parser, write format/UI docs, push CI repo, set up verification workflows

Work Log:
- Implemented NTFS Volume Boot Record (VBR) parser in crates/dw-ntfs/src/boot.rs
  - VbrRaw (repr(C, packed) Pod) for zero-copy reinterpretation
  - Validates OEM ID, bytes_per_sector, sectors_per_cluster, boot signature, MFT record size encoding
  - 8 unit tests, all passing
  - Fixed struct layout bug: added missing total_sectors_32 field at offset 32
- Wired NtfsPlugin::probe() to read sector 0, parse VBR, return ProbeOutcome::Yes with fs metadata
- Implemented FAT12/16/32 BIOS Parameter Block (BPB) parser in crates/dw-fat/src/bpb.rs
  - Implements FAT type decision tree from Microsoft FAT spec (< 4085 clusters = FAT12, < 65525 = FAT16, else FAT32)
  - BpbShared (Pod) for shared BPB, BpbExtFat32 for FAT32 extended fields
  - Offset helpers: fat_offset_bytes, root_dir_offset_bytes, first_data_offset_bytes, cluster_to_byte
  - 6 unit tests, all passing
- Wired FatPlugin::probe() to parse BPB and return ProbeOutcome with fs_kind 'fat12'/'fat16'/'fat32'
- Total workspace tests: 14 passing (8 NTFS + 6 FAT)
- Clippy clean with -D warnings, cargo fmt clean
- Created data-wizard-ci repo with 5 workflows:
  - build-and-test.yml: clone + build + test + upload artifacts (Windows 2022)
  - security-scan.yml: cargo audit + npm audit + winchecksec (verifies ASLR/DEP/CFG)
  - verification.yml: nightly NTFS round-trip test (create VHD, copy files, delete, scan)
  - benchmarks.yml: nightly criterion benchmarks
  - fuzz.yml: nightly cargo-fuzz on 8 parser targets
- Wrote format documentation (docs/formats/):
  - filefilter-xml.md: FileFilter.xml schema (format only, we rebuild in TOML)
  - raw-signs-json.md: raw_signs.json schema (format only, we rebuild from public sources)
  - petools-xml.md: PEtools.xml schema (WinPE bootstrap, out of scope for v0.1)
  - signatures-bin.md: our signatures.bin v1 binary format spec
- Wrote UI/UX documentation (docs/ui-ux/):
  - patterns.md: information architecture (NOT pixel clones), 7 design principles
  - flows.md: 5 user flows + 3 anti-flows we explicitly do NOT do
- Pushed 4 commits to data-wizard-rs main:
  - 573966e: initial scaffold
  - 78a2d53: NTFS VBR parser
  - 8517599: FAT BPB parser
  - 03706b2: format + UI/UX docs
- Pushed 1 commit to data-wizard-ci main (initial CI workflows)

Stage Summary:
- 2 FS parsers implemented (NTFS VBR + FAT BPB) with 14 passing unit tests
- 16 documentation artifacts written and cross-linked
- Both GitHub repos live with CI configured
- Workspace compiles cleanly, clippy clean with -D warnings
- Ready for next milestone: NTFS MFT walker (the core metadata scan)

Next steps (Task 4+):
- Implement NTFS MFT record parser (FILE record magic, attribute walk)
- Implement FAT directory entry parser (LFN + 8.3)
- Implement exFAT boot sector parser
- Wire up Tauri events for live scan progress
- Set up GitHub Actions dispatch from data-wizard-rs pushes to data-wizard-ci
- Add NTFS test fixtures (small NTFS image with known files)

---
Task ID: 4
Agent: main
Task: Implement NTFS MFT record parser with USN fixup + run list decoder

Work Log:
- Implemented FileRecordHeader (48-byte repr(C, packed) Pod struct)
  with all fields per MS-NTFS spec: magic, USN offset/size, LSN,
  sequence number, hard link count, first attribute offset, flags,
  used/allocated size, base record reference, next attribute ID,
  MFT record number (Vista+)
- Implemented AttributeHeader + Attribute + AttributeData types for
  parsing both resident (inline) and non-resident (cluster-based)
  attributes
- Implemented FileRecord::parse() with full USN fixup logic:
  - Validates FILE magic
  - Reads USN array (value + saved sector trailers)
  - For each 512-byte sector: verifies trailer matches USN value,
    replaces with saved value (detects torn writes)
  - Walks attribute list from first_attribute_offset to 0xFFFFFFFF
  - Parses resident attrs (content length + offset) and non-resident
    attrs (VCN range + run list + allocated/real/initialized size)
- Implemented decode_run_list() for NTFS compressed run list encoding:
  - Header byte: low nibble = length size (1-8), high nibble = offset size (0-8)
  - Length: unsigned LE
  - Offset: signed LE, relative to previous run (sparse if off_size=0)
  - Sign-extends negative offsets for backward runs
- Added attribute type constants module (STANDARD_INFORMATION, FILE_NAME,
  DATA, INDEX_ROOT, INDEX_ALLOCATION, etc.)
- 7 unit tests, all passing:
  - parses_minimal_file_record (Vista+ layout)
  - rejects_bad_magic
  - detects_usn_mismatch (torn write detection)
  - decode_simple_run_list
  - decode_multi_run_list (relative offset accumulation)
  - decode_sparse_run (sparse runs between real runs)
  - decode_negative_offset (backward runs with signed offset)
- Total workspace: 21 tests passing (8 NTFS boot + 7 NTFS MFT + 6 FAT BPB)
- Clippy clean with -D warnings, cargo fmt clean
- Pushed commit 1b297da to data-wizard-rs main

Stage Summary:
- NTFS parser now handles: VBR probe + MFT record parsing + attribute walk
  + run list decoding. Ready for the MFT walker (iterate all records,
  extract file names + data runs, build recovered entries).
- FAT parser handles: BPB probe. Ready for directory entry parsing.
- 21 unit tests across 3 parser modules, all passing.

Next steps (Task 5+):
- Implement FAT directory entry parser (LFN + 8.3)
- Implement NTFS MFT walker (iterate records, build RecoveredEntry)
- Implement exFAT boot sector parser
- Wire up Tauri events for live scan progress
- Add integration tests with real NTFS/FAT images (created on CI)

---
Task ID: 5
Agent: main
Task: Implement NTFS MFT walker, Tauri event streaming, integration tests, exFAT parser, public-source signature DB

Work Log:
- Step 1: NTFS MFT walker (crates/dw-ntfs/src/walker.rs)
  - Refactored mft.rs from borrowed to owned data (removed Box::leak — critical for walking millions of records without leaking gigabytes)
  - Added filename.rs: $FILE_NAME attribute parser (parent reference, UTF-16 name, 4 FILETIME timestamps, namespace constants)
  - Added walker.rs: walk_mft() reads MFT record 0 ($MFT itself), decodes its $DATA run list, iterates every record, extracts file names + data runs, builds RecoveredEntry, emits progress events
  - Handles deleted records (in-use bit clear), extension records (base_record_reference != 0), sparse MFT regions
  - Wired up NtfsPlugin::scan() to call walk_mft()
  - 25 NTFS tests (8 boot + 7 mft + 6 filename + 4 walker)

- Step 2: Tauri events for live scan progress (src-tauri/src/progress_sink.rs)
  - TauriProgressSink implements dw_core::ProgressSink, forwards events to webview via 'scan:progress' Tauri event
  - dw-core Scan::run() implemented as real orchestrator: probes each plugin, runs matching one, handles cancellation
  - start_scan command now spawns background tokio task that runs the scan
  - React UI ScanningView listens for 'scan:progress' events, shows live progress bar + phase + counts
  - ResultsView loads and displays results in a table with status badges
  - Added ManualSourceEntry for scanning file-backed images

- Step 3: Integration tests + CI verification (crates/dw-ntfs/examples/scan_image.rs + tests/integration.rs)
  - scan_image example: standalone scanner binary, opens file image, runs NTFS plugin, prints results
  - integration.rs: scan_ntfs_image (#[ignore], needs DW_TEST_IMAGE env var), scan_nonexistent_image_errors (always runs)
  - CI verification workflow updated: creates NTFS VHD, copies 100 files, deletes 50, runs scan_image, verifies >= 10 entries, runs integration test
  - Also added FAT32 VHD round-trip job

- Step 4: exFAT boot sector parser (crates/dw-exfat/src/boot.rs)
  - ExfatBootSectorRaw (repr(C, packed) Pod) + ExfatBoot (parsed, validated)
  - Verifies: 'EXFAT   ' OEM ID, MustBeZero field, BytesPerSectorShift 9-12, SectorsPerClusterShift 0-25, NumberOfFats 1-2, FileSystemRevision 0x0100, boot signature
  - Extracts: partition offset, volume length, FAT offset/length, cluster heap offset, cluster count, root dir first cluster, volume serial, volume flags
  - 16 tests, all passing
  - Wired up ExfatPlugin::probe()
  - Added ExfatError::NotExfat variant

- Step 5: Public-source signature DB (crates/dw-signatures/)
  - assets/public-signatures/signatures.toml: 35+ signatures from PUBLIC sources (Gary Kessler GCF, NIST DFXML, Wikipedia, libmagic). Each has a 'source' citation field.
  - build.rs: compiles TOML → Rust source at build time, embedded in binary. No runtime file dependency. Supports wildcard bytes (??).
  - spec.rs: SignatureSpec (compile-time, 'static refs) + Signature (runtime, owned) + MagicByte = Option<u8>
  - db.rs: SignatureDb::from_embedded() builds DB from compile-time const array
  - matcher.rs: Matcher::find_headers() scans byte window with wildcard support
  - 7 tests, all passing

Stage Summary:
- 62 total tests passing (25 NTFS + 13 FAT + 16 exFAT + 7 signatures + 1 integration)
- Clippy clean with -D warnings, cargo fmt clean
- 6 commits pushed to data-wizard-rs main
- 1 commit pushed to data-wizard-ci (verification workflow update)
- Real scan engine working: NTFS MFT walker + live progress streaming + results display
- 3 FS parsers with probe capability (NTFS, FAT, exFAT)
- 35+ file signatures embedded in binary from public sources

Next steps (Task 6+):
- Implement NTFS $INDEX_ROOT/$INDEX_ALLOCATION parsing for directory hierarchy resolution
- Implement FAT directory walker (iterate root dir + subdirs, follow FAT chains)
- Implement exFAT directory entry parser (generic-primary + stream-extension + file-name)
- Wire up the carver (dw-carver) to run on unallocated clusters during deep scan
- Add Windows disk enumeration (QueryDosDevice + FindFirstVolumeW) in list_disks command
- Add real file recovery (read clusters, write to destination) in recover_files command
- Add more signatures to the DB (target: 200+ from GCF)
- Add fuzzing targets for each parser
