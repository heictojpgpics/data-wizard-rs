# User Flows

## Flow 1: Recover accidentally-deleted files (most common)

```
User opens Data Wizard
       │
       v
App enumerates disks + partitions
       │
       v
User picks the partition where files were deleted
       │
       v
User clicks "Quick scan"  ──────────>  App runs metadata walk
       │                                     │
       │                                     v
       │                              Files stream into UI live
       │                                     │
       v                                     v
User browses results, filters by type/size/date
       │
       v
User selects files, clicks "Recover to..."
       │
       v
App prompts for destination directory (must be on a DIFFERENT drive)
       │
       v
App writes files, verifies checksums, shows success summary
```

## Flow 2: Recover files from a formatted drive

```
User opens Data Wizard
       │
       v
User picks the formatted partition
       │
       v
User clicks "Deep scan"  ──────────>  App runs metadata walk
       │                                    (finds nothing — FS was
       │                                     formatted)
       │                                     │
       │                                     v
       │                              App falls back to raw carving
       │                                     │
       │                                     v
       │                              Carved files stream into UI
       │                              (status: Carved, was_deleted: true)
       │                                     │
       v                                     v
User browses results, previews files to verify integrity
       │
       v
User selects files, recovers to a different drive
```

## Flow 3: Recover files from a raw disk image (forensics use case)

```
User opens Data Wizard
       │
       v
User clicks "Browse for file image"
       │
       v
App prompts for .img / .dd / .raw file
       │
       v
App opens the image as a FileBackedSource
       │
       v
User clicks "Deep scan"
       │
       v
(same as Flow 2 from here)
```

## Flow 4: Resume a saved scan

```
User opens Data Wizard
       │
       v
User clicks "Open saved scan..."
       │
       v
App prompts for .dwscan file
       │
       v
App loads the scan state + results
       │
       v
User continues browsing / recovering / resuming the scan
```

## Flow 5: Cancel a long-running scan

```
User clicks "Cancel scan"
       │
       v
App sets cancel flag
       │
       v
Scanner checks flag at next progress emission (within ~50ms)
       │
       v
Scanner returns CoreError::Cancelled
       │
       v
App preserves partial results, returns to results view
       │
       v
User can browse/recover the partial results OR start a new scan
```

## Anti-flows (things we explicitly do NOT do)

### Anti-flow A: "Upgrade to Pro" prompt

```
(We do NOT do this. Ever.)
```

The reference product nags the user with upgrade prompts at every
opportunity. We do NOT. Every feature is free. Every user gets every
feature. No paywall, no trial, no "Pro version".

### Anti-flow B: Telemetry opt-in dialog on first launch

```
(We do NOT do this by default.)
```

If we ever add opt-in crash reporting, it will be in the Settings page,
not a modal dialog on first launch. The default is OFF.

### Anti-flow C: Forced update check

```
(We do NOT do this in v0.1.)
```

Updates are manual (user clicks "Check for updates" in Settings). We
will add signed automatic updates later, but they will always be opt-in.
