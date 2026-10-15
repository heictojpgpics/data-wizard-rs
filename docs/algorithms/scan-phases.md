# Scan Phases

## Reference product (observed via UI behaviour)

When the user picks a source and clicks "Scan", the reference product runs
through these phases (visible from the UI's progress indicator):

1. **Probing** (~1–3 seconds)
   - Open the source.
   - Read sector 0 (boot sector).
   - Try each FS parser's probe in turn: NTFS, FAT, exFAT, EXT4, HFS+, Btrfs,
     ReFS, ZFS.
   - Pick the first parser that recognises the boot sector.
   - If none recognises it, fall back to RAW carving only.

2. **Quick scan** (seconds to minutes, depending on disk size)
   - Walk the filesystem's own metadata:
     - NTFS: walk the MFT directly.
     - FAT: walk the root directory + FAT chains.
     - exFAT: walk the root directory + allocation bitmap.
     - ext: walk each block group's inode table.
   - Surface deleted-but-not-overwritten entries.
   - Update the UI live as entries are found.

3. **Deep scan** (minutes to hours)
   - Continue the metadata walk if not finished.
   - For every cluster NOT claimed by a live filesystem entry:
     - Scan for file headers (raw_signs.json).
     - For each header hit, run the matching "analyser" to validate.
     - If valid, emit a carved entry with `was_deleted=true` and `status=Carved`.
   - Update the UI with a progress bar (current cluster / total clusters).

4. **Finalizing** (~1–5 seconds)
   - Sort results by path / size / type.
   - Build any indexes the UI needs (filter by extension, by date, etc.).
   - Mark the scan as `Done`.

## Our scan lifecycle

We mirror the same phases, expressed as a state machine in
[`dw-core::scan`](../../crates/dw-core/src/scan.rs):

```rust
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum ScanState {
    Created,
    Probing,
    QuickScan,
    DeepScan,
    Finalizing,
    Done,
    Cancelled,
    Failed,
}
```

### State transitions

```
Created ──> Probing ──> QuickScan ──> DeepScan ──> Finalizing ──> Done
   │           │            │             │             │
   │           v            v             v             v
   └──────> Failed <───────Failed <──────Failed <─────Failed
                │
                v
            Cancelled (user-initiated only)
```

### Cancellation

Cancellation is cooperative. The UI calls a `cancel_scan` Tauri command,
which sets a flag in the `ScanHandle`. Each plugin checks the flag at
every progress emission (every 1024 MFT records, every 1 MiB of carving,
etc.) and returns `CoreError::Cancelled` if set.

This means a scan can take up to ~50 ms to actually stop after the user
clicks "Cancel" — fast enough to feel responsive, slow enough to avoid
checking the flag on every byte.

### Progress reporting

Progress is reported via `tokio::sync::broadcast` channel (in-process) and
forwarded to the UI via Tauri events:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanProgressEvent {
    pub scan_id: String,
    pub phase: ScanPhase,
    pub current: u64,
    pub total: u64,
    pub message: String,
}
```

- `current` and `total` are phase-dependent:
  - Probing: 0..1 (binary "done" signal)
  - QuickScan: current MFT record / total MFT records (NTFS), or current
    inode / total inodes (ext)
  - DeepScan: current cluster / total clusters
  - Finalizing: 0..1
- The UI uses `current / total` to render a progress bar and ETA.
- `message` is a human-readable string for the current sub-task
  (e.g. "Walking MFT record 1234567/4194304").

### Resume (planned)

A scan can be persisted to disk (`SaveScan/` in the reference product). We
will support this in a follow-up — the on-disk format will be postcard-
encoded `ScanState` + a `ResultsStore` snapshot. This lets the user close
the app mid-scan and resume later without re-walking the MFT.

### Error recovery

If a plugin errors mid-scan (e.g. corrupt MFT record), the orchestrator
logs the error, marks the current entry as `PartiallyRecovered`, and
continues. The scan only fails entirely if the source becomes unreadable
(disk disconnected, I/O error).

This matches the reference product's behaviour — a single corrupt record
doesn't kill the scan.
