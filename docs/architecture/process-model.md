# Process Model

## Reference product (observed)

Four cooperating Windows processes:

### 1. `DRW.exe` (124 KB, amd64 GUI, **no ASLR**)

A thin launcher. Its job is to:

1. Parse command-line arguments (URL handlers, file associations).
2. Detect whether to run in normal mode or WinPE mode (checks for the
   `SystemBoot` env var or the presence of `winpeshl.ini`).
3. Spawn `DRWService.exe` (elevated, if needed) — this holds the raw disk
   handles.
4. Spawn `DRWUI.exe` (the actual Qt5 UI).
5. Wait for `DRWUI.exe` to exit, then shut down the service.

**Security observation**: `DRW.exe` does NOT have ASLR enabled
(`DllCharacteristics & 0x0040 == 0`). This is unusual for a 2026-era binary
and is a weakness — an attacker who can hijack the launcher can rely on
statically-located gadgets. We will NOT replicate this; our launcher will
have full ASLR/DEP/CFG.

### 2. `DRWUI.exe` (29 MB, amd64 GUI)

The main UI process. Qt5-based (imports `Qt5Core`, `Qt5Gui`, `Qt5Widgets`,
`Qt5Network`, `Qt5Svg`, `Qt5WinExtras`). 49 imported DLLs, 3610 imported
functions, 9 exports.

Talks to `DRWService.exe` over **ZeroMQ** (observable: `libzmq-mt-4_3_5.dll`
is loaded by both processes). ZeroMQ gives them fast in-process or
TCP-style IPC without the boilerplate of named pipes.

Also imports `dbghelp.dll` — likely for in-app crash dumps.

### 3. `DRWService.exe` (39 KB, amd64 GUI)

The privileged service. Holds raw disk handles (`CreateFileW` on
`\\.\PhysicalDriveN` and `\\.\X:` with `FSCTL_ALLOW_EXTENDED_DASD_IO`).

Only 10 imported DLLs / 93 imported functions — very small surface area,
which is good. The service is the only process that needs admin privileges;
the UI runs as the regular user.

### 4. `DRWReport.exe` (4.6 MB, amd64 GUI, **no ASLR**)

Generates scan-result reports (probably exports a list of recoverable files
to HTML/PDF/CSV). Imports `Qt5` for rendering. Also lacks ASLR.

## Our model (data-wizard-rs)

### Phase 1 (current): single Tauri process

For the initial release we collapse everything into a single Tauri 2
process. The Rust core lives in the same process as the WebView2 UI.

```
+----------------------------------+
|   data-wizard.exe (Tauri 2)      |
|                                  |
|   +------------+  +----------+   |
|   | WebView2   |  | Rust     |   |
|   | (React/TS) |<>| core     |   |
|   +------------+  +----------+   |
|                        |         |
|                        v         |
|              DeviceIoControl     |
|              (raw disk I/O)      |
+----------------------------------+
```

**Why single process for now:**

- Simpler trust boundary. Easier to audit. Smaller attack surface.
- Tauri 2's IPC is already type-safe (`tauri-specta`).
- We only need elevation when the user picks a physical drive; we'll
  prompt UAC at that point.
- The reference product's split exists primarily to allow the UI to run
  unelevated while the service runs elevated. We can achieve the same
  by running the whole app unelevated and spawning an elevated helper
  only for raw-disk reads (Phase 2).

**Why we'll split later:**

When we add **Crashed PC Rescue** (boot from USB, scan the unbootable
system disk), we'll need:

1. A bootable WinPE-like image with our recovery binary.
2. A privileged helper service that runs as SYSTEM and owns the raw
   disk handles.
3. The UI runs as a regular user and talks to the helper over IPC.

At that point we'll mirror the reference product's split. Until then,
the complexity isn't justified.

### Phase 2 (future): privileged helper

```
+--------------------------------+      +-----------------------------+
| data-wizard.exe (Tauri 2)      |      | dw-helper.exe               |
| (unelevated, regular user)     |      | (elevated, runs as SYSTEM)  |
|                                |      |                             |
| +----------+   +-----------+   |      | +-------------------------+ |
| | WebView2 |<> | Rust core |<--+------| | Raw disk handle cache  | |
| +----------+   +-----------+   | IPC  | | (DeviceIoControl)       | |
|                                |      | +-------------------------+ |
+--------------------------------+      +-----------------------------+
```

IPC will be a named pipe with explicit ACL (only the same user can connect).
Messages will be length-prefixed postcard frames, type-safe via `dw-ipc`.

### Phase 3 (future): bootable rescue image

A WinPE-like bootable USB image containing our recovery binary. Built
from a Windows ADK base + our binary + a minimal boot configuration.

Out of scope for the initial release.
