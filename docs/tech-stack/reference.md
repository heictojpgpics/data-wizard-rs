# Reference Product Tech Stack (observable)

Derived from the DLLs present in the merged payload. We document this to
understand the engineering choices of the reference product; we make
different choices for our product (see
[`ours.md`](ours.md)).

## Language & runtime

- **C++** (the FS parsers, the core, and the UI are all native C++ — no
  .NET managed assemblies were observed in the core path; only
  `EuOfficeRepairWin32.exe` is i386, suggesting some legacy code).
- **Qt 5** for the UI (not Qt 6 — observable from `Qt5Core.dll`,
  `Qt5Gui.dll`, `Qt5Widgets.dll`, `Qt5Network.dll`, `Qt5Svg.dll`,
  `Qt5WinExtras.dll`). Qt 5 reached end-of-life in 2023 but is still
  shipped.
- **MSVC runtime**: bundles `msvcr90`, `msvcp90` (VC++ 2008),
  `msvcr120`, `msvcp120` (VC++ 2013), `msvcp140`, `vcruntime140`,
  `concrt140` (VC++ 2015-2022). Mixed runtime versions suggest gradual
  accumulation of dependencies over years.
- **UCRT** (`ucrtbase.dll`) — the Universal C Runtime.

## Concurrency & IPC

- **ZeroMQ** (`libzmq-mt-4_3_5.dll`) — used for IPC between `DRWUI.exe`
  and `DRWService.exe`. Good choice for fast in-process or TCP-style
  messaging.
- **C++ REST SDK** (`cpprest141_2_10.dll`) — Microsoft's `cpprestsdk`
  v2.10, built against VC++ 2017. Used for HTTP/JSON async work
  (likely the Firebase and Aliyun cloud integrations).

## Crypto

Three OpenSSL versions bundled simultaneously:

- **OpenSSL 1.0.x** (`libeay32.dll`, `ssleay32.dll`, `libeay32MD.dll`,
  `ssleay32MD.dll`) — very old, EOL 2016.
- **OpenSSL 1.1.x** (`libcrypto-1_1.dll`, `libssl-1_1.dll`,
  `libcrypto-1_1-x64.dll`, `libssl-1_1-x64.dll`) — EOL 2023.
- **OpenSSL 3.x** (`libcrypto-3.dll`, `libcrypto-3-x64.dll`,
  `libssl-3-x64.dll`) — current.

This is a sign of accumulated dependencies — different libraries pull
in different OpenSSL versions. A security audit would flag this.

Also `libssh2.dll` for SSH (likely the NAS recovery feature — out of
scope for us).

## Image / video / document processing (file-repair modules)

- **ImageMagick** (`CORE_RL_MagickCore_.dll`, `CORE_RL_glib_.dll`,
  `CORE_RL_cairo_.dll`, `CORE_RL_pango_.dll`, `CORE_RL_rsvg_.dll`) —
  i386, for the image repair module.
- **FFmpeg** (`avformat-62.dll` and others) — i386, for the video
  repair module.
- **libx265** / **libde265** / **libheif** — HEIF/HEIC decoding.
- **pdfium** (`pdfium.dll`) — PDF rendering for the in-app PDF viewer.
- **GDI+** (`GDIPLUS.DLL`) — Windows GDI+ for legacy image handling.

## Localization

114 `.qm` Qt translation files. The product supports a large number of
languages.

## Telemetry / cloud

- **Firebase** (`firebasefetch.exe`, `firebaseonlineconfig.dll`,
  `google-services.json`) — Firebase remote config & analytics.
- **Aliyun OSS** (`AliyunWrap.dll`, `AliyunWrap.dll`,
  `AliyunWrapExe.exe`, `AliyunConfig.ini`) — Chinese cloud storage,
  used for crash dump upload (`send_dmp_with_aliyun.exe`) and likely
  the "Cloud Recovery" feature.

## Installer

- **NSIS** (Nullsoft Scriptable Install System) — observable from the
  `easeus_setup.exe` PE header ("Nullsoft Installer self-extracting
  archive"). NSIS is a reasonable choice for a Windows installer.
- **Apple DMG** for macOS (zlib-compressed).

## Bootable rescue environment

- **WinPE** (Windows Preinstallation Environment) — observable from
  `BUILDPE/x64/Windows/system32/winpeshl.ini` and `Unattend.xml`.
- Custom **kernel drivers** (`EUDCPEPM.sys`, `EUEDKEPM.sys`,
  `ebrntdrv.sys`, `epmdkdrv.sys`) — signed, used for raw disk access
  when booted into WinPE.
- **Windows AIK** (Automated Installation Kit) tools — observable from
  `PEtools.xml` (bcdboot, imagex, wimmount, oscdimg, dism).

## What we use instead (summary)

See [`ours.md`](ours.md) for the full rationale. Summary:

| Layer | Reference | Ours |
|---|---|---|
| Language | C++ | Rust (memory-safe) |
| UI framework | Qt 5 (EOL) | React 19 + Tauri 2 WebView2 |
| IPC | ZeroMQ | tauri-specta (type-safe) |
| Crypto | 3 OpenSSL versions | RustCrypto crates (single version) |
| Image/video | ImageMagick + FFmpeg (i386) | Not in v0.1; later via GStreamer bindings |
| Localization | Qt .qm files | i18next + JSON |
| Telemetry | Firebase + Aliyun | None (opt-in crash dump to local file) |
| Installer | NSIS | Tauri's MSI + NSIS bundlers |
| Rescue | WinPE + custom drivers | Out of scope for v0.1 |
