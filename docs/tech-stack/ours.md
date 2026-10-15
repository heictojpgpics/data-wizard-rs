# Our Tech Stack (and rationale)

All versions are the latest stable as of 2026-10.

## Core

| Component | Choice | Version | Why |
|---|---|---|---|
| Language | Rust | 1.99 (stable) | Memory safety, zero-cost abstractions, no GC pauses during deep scans |
| Edition | Rust 2024 | — | Latest edition, async closures, `let-else` stabilised |
| Async runtime | tokio | 1.47 | Industry standard, full features for I/O + sync + signals |
| HTTP | not bundled in v0.1 | — | No network code in the core. Telemetry is opt-in and out-of-process. |
| Logging | tracing + tracing-subscriber | 0.1 / 0.3 | Structured, async-aware, layered (file + console + JSON) |

## Filesystem parsers (one crate each, modular)

| Crate | Purpose | Public spec source |
|---|---|---|
| `dw-ntfs` | NTFS 3.x | MS-NTFS Open Specifications, Linux `fs/ntfs3/`, libntfs-3g |
| `dw-fat` | FAT12/16/32 | Microsoft FAT spec, Linux `fs/fat/` |
| `dw-exfat` | exFAT | Microsoft exFAT spec, Linux `fs/exfat/` |
| `dw-ext` | ext2/3/4 | `e2fsprogs`, Linux `fs/ext4/` |
| `dw-hfs` | HFS+ | Apple Technote 1150, Linux `fs/hfsplus/` |
| `dw-btrfs` | Btrfs | wiki.btrfs, Linux `fs/btrfs/` |
| `dw-refs` | ReFS v3.x | Microsoft docs, WDK samples |
| `dw-zfs` | ZFS | OpenZFS source, on-disk format docs |
| `dw-signatures` | File signature DB | Gary Kessler GCF, NIST DFXML, PRONOM, libmagic |
| `dw-carver` | Raw carving | Foremost/Scalpel/Photorec, Richard & Roussev 2006 |

## UI / app shell

| Component | Choice | Version | Why |
|---|---|---|---|
| App shell | Tauri | 2.x | Native WebView2, no Electron bloat, type-safe IPC |
| Frontend | React | 19 | Latest stable, concurrent rendering, actions |
| Language | TypeScript | 5.7 | Strict mode, no `any` |
| Build | Vite | 7 | Fast HMR, ES2022 output, small bundle |
| Styling | Tailwind CSS | 4 | Utility-first, no CSS-in-JS runtime cost |
| State | Zustand | 5 | Small, ergonomic, no boilerplate |
| Data fetching | TanStack Query | 5 | Cache, retries, background refetch |
| Virtualization | TanStack Virtual | 3 | For the million-row results list |
| IPC types | tauri-specta | 2.0-rc | Auto-generated TS bindings from Rust types |

## Tauri plugins

| Plugin | Purpose |
|---|---|
| `tauri-plugin-dialog` | File/folder pickers (destination dir) |
| `tauri-plugin-fs` | Scoped filesystem access (destination dir only) |
| `tauri-plugin-shell` | Open recovered file in Explorer |
| `tauri-plugin-log` | Rust → UI log forwarding |
| `tauri-plugin-notification` | OS notifications (scan done) |
| `tauri-plugin-opener` | Open URLs/files |
| `tauri-plugin-process` | App lifecycle (restart, exit) |
| `tauri-plugin-window-state` | Persist window size/position |

Deliberately NOT using:
- `tauri-plugin-http` — no network code in v0.1
- `tauri-plugin-store` — we use our own scan-state format
- `tauri-plugin-updater` — we'll add later with signature verification
- `tauri-plugin-deep-link` — not needed yet

## Build & release

| Component | Choice | Version |
|---|---|---|
| Build | cargo + tauri build | latest |
| Release targets | MSI + NSIS + portable ZIP | Tauri's bundlers |
| MSIX | planned for later | Tauri's MSIX bundler |
| Code signing | planned | signtool.exe with EV cert |

## CI / verification

| Component | Choice | Why |
|---|---|---|
| Runner | `windows-2022` GitHub Actions | Microsoft-hosted, latest Windows Server |
| Rust cache | Swatinem/rust-cache@v2 | Saves ~5 min on every build |
| npm cache | setup-node@v5 cache | Saves ~30 sec |
| Artifacts | upload-artifact@v4 | 14-day retention for builds, 90-day for releases |
| Fuzz | cargo fuzz + nightly | Nightly job, long runs |
| Security scan | cargo-audit + winchecksec | Every PR |
| Lint | clippy + eslint | Every PR, deny warnings |

See [`data-wizard-ci`](https://github.com/heictojpgpics/data-wizard-ci) for
the verification repo (separate to keep build logs isolated from the
source repo).

## Dependency policy

- All dependencies are pinned to the latest stable semver at the time
  of each release.
- `Cargo.lock` and `package-lock.json` are committed.
- `cargo audit` and `npm audit` run in CI on every PR.
- Major version bumps require a PR review (we don't auto-merge
  dependabot major bumps).
- No third-party proprietary code. No third-party telemetry. No
  third-party analytics.

## What we explicitly do NOT use

| Thing | Why |
|---|---|
| Electron | 100+ MB memory overhead; we use Tauri's WebView2 |
| Qt | Cross-platform but C++ and heavy; we use React |
| OpenSSL | Three versions in the reference product; we use RustCrypto |
| Firebase / Aliyun / Sentry | No telemetry by default |
| ImageMagick | i386 only in reference; we'll add image repair later via Rust crates |
| FFmpeg | i386 only in reference; we'll add video repair later via GStreamer bindings |
| ZeroMQ | We use tauri-specta IPC (single-process) |
| C++ REST SDK | We're not doing HTTP in the core |
