# data-wizard-rs

State-of-the-art data recovery wizard for Windows, built from scratch in **Rust + Tauri 2 + React/TypeScript**.

> Clean-room re-implementation. We studied the publicly observable architecture of commercial data-recovery products (process model, filesystem-parser-per-FS modular design, raw carving approach, scan-state persistence, UI flow patterns) and rebuilt every line ourselves. **No third-party code, assets, strings, icons, or UI elements are borrowed.** No license or activation is bypassed. Our product is free and open-source under MIT OR Apache-2.0.

## Project status

🚧 **Core engines working and tested; app wiring complete.** See the support matrix below for exactly what works today.

See [`worklog.md`](./worklog.md) for the running work log and [`docs/`](./docs/) for the full reverse-engineering documentation.

### Filesystem support matrix (honest, as of 2026-10)

| Filesystem | Quick scan | Deep scan (unallocated carving) | Status |
|---|---|---|---|
| NTFS 3.x | MFT walk, timestamps, resident data, extents | $Bitmap-guided carve + $I30 names for lost records | **Working, tested** |
| FAT12/16/32 | dir walk, FAT timestamps, extents | free-run carve | **Working, tested** |
| exFAT | entry-set walk, timestamps, fragmentation | allocation-bitmap carve | **Working, tested** |
| HFS+ | catalog B-tree walk, extents | free-block carve | **Working, tested** |
| ext2/3/4 | live walk + shadow-dirent deleted names (extent trees and indirect maps, fragmented files in content order) | orphan-inode pass + free-block carve | **Working, tested** |
| Btrfs | live subvolume walks (checksummed nodes, non-identity chunk mapping, fragmented order, inline files) | orphan-tree-block pass (old COW generations surface with names and extents) | **Working, tested** |
| ZFS (OpenZFS on-disk) | live tree walk from the newest uberblock: labels fletcher4-verified, MOS -> root dataset -> FS tree, SA or legacy-znode timestamps, physical extents, symlink resident data; LZJB/GZIP/ZLE/LZ4/ZSTD reads under fletcher2/fletcher4/SHA-256 verification | previous-uberblock pass (second-newest txg: recently deleted entries keep names, timestamps, extents) | **Working, tested** |
| Unknown/blank | - | raw carve fallback (258 signatures, tiered validation) | **Working, tested** |

Nothing else is claimed. ReFS is **not implemented yet**: the
reference product does ship a real engine for it (verified during
clean-room RE - see `open-recovery-wizard/docs/08-driver-and-module-re.md`),
and ours is in flight the honest way: a real formatted ReFS sample
volume is built on CI (`refs-sample.yml`), the layout gets validated
empirically against it (the approach that produced the HFS+ engine),
and only then does the matrix row appear. Until then there is no stub
crate to find: absent means absent.

Btrfs scope, stated honestly: single-device, non-RAID volumes with
crc32c-verified structures; compressed extents surface as metadata
only (decompression is not in v1); multi-device/RAID falls back to raw
carving with the reason recorded, never a half-parse.

ZFS scope, stated honestly: single-device pools (one top-level disk or
file vdev); mirrors, RAID-Z, multi-vdev, gang blocks, and native
encryption fall back to raw carving with the reason recorded. Extents
are the on-disk DVA ranges - for compressed blocks that is the
byte-faithful physical representation; logical reconstruction of
compressed streams is a recovery-writer feature, not a scan-time
promise.

ext4 journal-based deleted-dirent recovery (jbd2) is likewise a
documented roadmap item, not a present feature.

### Safety properties

- Strict positional reads: short reads are typed errors, never zero-filled success
- Recovery writes: temp + fsync + atomic publish, no-clobber by default, collision policies, free-space preflight, source-not-destination guard
- Bounded results: memory-bounded store with disk spill, paged access, streaming checkpoints
- Carving: bounded worker pool, chunk-overlap so boundary-straddling headers are found exactly once, cooperative cancellation
- Capabilities: dialog/log/window-state only (no fs/shell/process plugin surface)

## Goals

1. **Match** the recovery capability of commercial data-recovery wizards for the Windows desktop case (NTFS / FAT / exFAT / ext2-4 / ReFS / raw carving).
2. **Exceed** it in: security posture (full ASLR/DEP/CFG, memory-safe core), UX (modern webview UI), and openness (MIT/Apache, no paywalled features).
3. **Stay Windows-only for now**. macOS (APFS / HFS+ / ExFAT) is a later milestone. RAID / NAS / Cloud / Crashed-PC / Disk-Health are explicitly out of scope until the core product is stable.

## Tech stack (all latest stable as of 2026-10)

| Layer | Choice | Why |
|---|---|---|
| Core engine | Rust 1.99 (edition 2024) | Memory safety, zero-cost abstractions, no GC pauses during deep scans |
| Process model | Single Tauri app (no service split yet) | Simpler trust boundary; we'll split into a privileged helper only when raw-disk access needs elevation |
| FS parsers | One `dw-{fs}` crate per filesystem | Mirrors the modular `*SearchDLL` pattern observed in the reference product — narrow public API, easy to test in isolation |
| IPC | `tauri-specta` (type-safe TS bindings) | End-to-end type safety between Rust and React |
| UI | React 19 + TypeScript + Vite + Tailwind 4 | Fast HMR, modern, small bundle |
| Window | Tauri 2 + WebView2 | No Electron bloat; native webview |
| Build / release | `cargo` + `tauri build` | Outputs MSI + NSIS + portable ZIP |
| CI | GitHub Actions (Windows runners) | Workflows in `.github/workflows/` (Build and Test, Security Scan, Nightly Verification, Fuzz, Benchmarks) |

## Repository layout

```
data-wizard-rs/
├── crates/
│   ├── dw-core/         # scan orchestration, results store, progress, scheduler
│   ├── dw-disk/         # raw disk I/O abstraction (Windows DeviceIoControl + file-backed)
│   ├── dw-ntfs/         # NTFS MFT parser + USN journal + attribute stream recovery
│   ├── dw-fat/          # FAT12/16/32 parser (directory chain + FAT walking)
│   ├── dw-exfat/        # exFAT parser (FAT + directory bitmap + stream extensions)
│   ├── dw-ext/          # ext2/3/4 parser (group descriptors + inode tables + extents)
│   ├── dw-hfs/          # HFS+ parser (B-tree catalog + extents overflow)
│   ├── dw-btrfs/        # Btrfs parser (checksummed b-trees + chunk mapping + COW orphan pass)
│   ├── dw-zfs/          # ZFS parser (verified labels/uberblocks, DMU + SA + zap, prev-txg deep pass)
│   ├── dw-signatures/   # file signature DB (rebuilt from public sources — NOT extracted)
│   ├── dw-carver/       # raw file carving engine (header/footer + bifragment gap)
│   └── dw-ipc/          # types shared between Rust core and Tauri/React
├── src-tauri/           # Tauri 2 shell + command handlers
├── ui/                  # React/TS frontend
├── docs/                # reverse-engineering documentation + design specs
├── scripts/             # build / release / analysis scripts
├── .github/workflows/   # CI (lint, test, build, release)
├── worklog.md           # running work log (append-only)
├── Cargo.toml           # workspace
└── README.md
```

## License

Dual-licensed under **MIT** or **Apache-2.0** — pick either.

## Acknowledgements

This project benefits from the publicly documented body of knowledge around filesystem internals (NTFS — libntfs / Linux kernel `fs/ntfs/`; FAT — Microsoft FAT spec; ext2/3/4 — `e2fsprogs`; HFS+ — Apple technote 1150; Btrfs — wiki.btrfs; ReFS — Microsoft docs; ZFS — OpenZFS), file-carving research (NIST CFRED, Gary Kessler's GCF file signatures, Foremost/Scalpel/Photorec), and the broader digital-forensics community. We do not use or redistribute any third-party proprietary binaries, signature databases, or UI assets.
