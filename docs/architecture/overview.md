# Architecture Overview

## Reference product (observable, not copied)

The reference product is structured as **4 cooperating Windows processes** plus
a **WinPE-based bootable rescue environment**:

```
                +-----------------------------+
                |        DRW.exe (124 KB)     |   <- thin launcher
                |  (no ASLR -- a weakness     |
                |   we will NOT replicate)    |
                +-------------+---------------+
                              |
                              v
        +---------------------+---------------------+
        |                                             |
+-------+-------+                            +--------+--------+
| DRWUI.exe     |                            | DRWService.exe  |
| (29 MB, Qt5)  |  <-- UI                    | (39 KB, Qt5)    |  <-- privileged
| GUI process   |                            | service process |
+---------------+                            +-----------------+
        |                                             |
        |  IPC: named pipes / ZeroMQ (libzmq)         |  raw disk handles
        |  (observable: libzmq-mt-4_3_5.dll)          |  (DeviceIoControl)
        v                                             v
+---------------+                            +-----------------+
| DRWReport.exe |                            | kernel drivers  |
| (4.6 MB, Qt5) |  <-- report generator      | EUDCPEPM.sys    |
| (no ASLR)     |                            | EUEDKEPM.sys    |
+---------------+                            | ebrntdrv.sys    |
                                             | epmdkdrv.sys    |
                                             +-----------------+
```

### Filesystem parsers — modular DLL design

Each filesystem gets its own DLL with a **2-export factory API**:

| DLL | Exports | Purpose |
|---|---|---|
| `NTFSSearchDLL.dll` | `EUCreateNTFSSearcher`, `EUCreateNTFSQuickSearcher` | NTFS full + quick |
| `FATSearchDLL.dll` | `EUCreateFATSearcher`, `EUCreateFATQuickSearcher` | FAT12/16/32 |
| `exFATSearchDLL.dll` | `EUCreateEXFATSearcher`, `EUCreateEXFATQuickSearcher` | exFAT |
| `EXT4SearchDLL.dll` | `EUCreateEXT4Searcher`, `EUCreateEXT4QuickSearcher` | ext2/3/4 |
| `HFSPlusSearchDLL.dll` | (same pattern) | HFS+ |
| `BtrfsSearchDLL.dll` | (same pattern) | Btrfs |
| `ReFSSearchDLL.dll` | (same pattern) | ReFS |
| `ZfsSearchDLL.dll` | (same pattern) | ZFS |
| `RAWSearchDLL.dll` | `EUCreateRAWSearcher`, `EUCreateRAWSearcherForCellTest`, `EUGetAnalyserList`, `EUGetAnalyserMap`, `EUGetAnalyserMultimap`, `GetRawUserInfoMap` | raw carving fallback (richer API — has "analysers" that validate carved chunks per file type) |

The "Quick" vs "Full" split is the cleanest idea here — and we mirror it in
our [`FsPlugin`](../../crates/dw-core/src/plugin.rs) trait:

- **Quick scan**: walks the filesystem's own metadata (MFT, FAT, inode table)
  to enumerate deleted-but-not-overwritten entries. Fast (seconds).
- **Deep scan**: quick scan + raw carving on unallocated regions. Slow
  (minutes to hours).

### Auxiliary modules

| Module | Purpose |
|---|---|
| `PE_Tools.dll` | PE file reconstruction. Has `pet_install_driver`, `pet_download_packet`, `pet_decompress_packet`, `pet_verify_packet` — used to bootstrap the WinPE rescue environment, NOT for runtime recovery. |
| `PartitionRecovery/` | Separate sub-product: `MainModule.dll`, `RecoveryModule.dll`, `CloneModule.dll`, `ToolsModule.dll`, `UILogic.dll`, `Boot.dll`, `BootDriver.dll`, `RawFixer.dll`, `PartitionRecovery.dll` — partition table rebuild + clone. |
| `diskhealth/` | SMART / disk health (separate sub-product). |
| `EUFileRepair/` | File-format repair: Office, Adobe, Archive, HWP, Image, Video, Raw. Bundles ImageMagick (`CORE_RL_*`), FFmpeg (`avformat-62.dll`), libcurl, libcrypto-3, cairo, pango, rsvg. |
| `VideoViewer/`, `PDFViewer/`, `EUOfficeViewer.exe` | In-app preview viewers (Qt5 + pdfium + FFmpeg). |
| `Dict/` | Dictionary (likely file-type signature dictionary for the RAW analyser). |
| `SaveScan/` | Saved scan-state format (resume scans across sessions). |
| `Resource/`, `ResourceDark/` | UI resources (light/dark themes) — we do NOT copy these. |
| `language/` | Localizations (114 `.qm` Qt translation files). |
| `cloud_session.dll`, `AliyunWrap.dll`, `firebaseonlineconfig.dll` | Cloud / telemetry. We replace with opt-in-only local logging. |

### WinPE rescue environment (`BUILDPE/`)

A complete WinPE boot image with:

- Custom signed kernel drivers: `EUDCPEPM.sys`, `EUDKEPM.sys`, `ebrntdrv.sys`, `epmdkdrv.sys`
  - These are the low-level disk access drivers used when booted into WinPE
    (the regular Windows kernel doesn't need them because it has its own
    disk driver stack).
- `BUILDPE/EaseUS-x64/` — full copy of the DRW application compiled for WinPE
- `BUILDPE/x64/Windows/system32/drivers/` — driver install directory
- `Unattend.xml`, `winpeshl.ini` — WinPE boot configuration

This is used for "Crashed PC Recovery" mode — boot the user's PC from a USB
stick containing this WinPE image, then run DRW from WinPE to recover files
from the unbootable system disk. **Out of scope for our initial release.**

## Our architecture (data-wizard-rs)

```
+----------------------------------------------------------+
|                Tauri 2 App (single process)              |
|                                                          |
|  +-------------------+    +---------------------------+  |
|  | React/TS UI       |    | Rust core                 |  |
|  | (WebView2)        |<-->| (tauri-specta IPC)        |  |
|  |                   |    |                           |  |
|  | - Disk selection  |    | - dw-core (orchestrator)  |  |
|  | - Scan progress   |    | - dw-disk (raw I/O)       |  |
|  | - Results browse  |    | - dw-ntfs / dw-fat / ...  |  |
|  | - Recover dialog  |    | - dw-carver               |  |
|  +-------------------+    | - dw-signatures           |  |
|                           +---------------------------+  |
|                                          |               |
|                          +---------------+---------------+
|                          | Windows DeviceIoControl       |
|                          | (raw physical drive access)   |
|                          +-------------------------------+
+----------------------------------------------------------+
```

### Why we collapse to a single process (for now)

The reference product splits UI and service into separate processes, primarily
to allow the service to run elevated (admin) while the UI runs as the regular
user. This is good defense-in-depth but adds complexity (IPC, state sync).

For our **initial release** we run as a single Tauri process and request UAC
elevation only when the user picks a source that requires raw disk access
(physical drives; logical volumes that are in use). When we add the
**Crashed PC Rescue** feature (later), we'll introduce a privileged helper
service and a separate WinPE bootable image — at that point we'll mirror the
reference product's split.

### Crate-level dependency graph

```
                    dw-ipc (types)
                       ^
                       |
            +----------+----------+----------+
            |                     |          |
        dw-core               src-tauri      ui (TS)
        ^     ^
        |     |
   dw-disk  dw-carver -> dw-signatures
        ^
        |
   +----+----+----+----+----+----+----+----+
   |    |    |    |    |    |    |    |
  ntfs fat exfat ext hfs btrfs refs zfs
```

Every FS parser implements [`dw_core::FsPlugin`](../../crates/dw-core/src/plugin.rs).
The orchestrator (`dw-core::Scan`) probes each plugin in turn, picks the first
that claims the region, and runs quick/deep scan.
