# Licensing Boundary

## Important: clean-room only

We do **NOT** bypass, patch, or emulate the reference product's license
check. We document here only what is **publicly visible** to anyone who
downloads the free version — this is the same information that appears on
their marketing page.

## Reference product (publicly visible feature gates)

The free version of the reference product (`recoveryFree.ini`, observable
in the install directory) gates the following features behind a paid
license:

| Feature | Free tier | Paid tier |
|---|---|---|
| Recover up to 2 GB | Yes | Unlimited |
| Quick scan (metadata walk) | Yes | Yes |
| Deep scan (raw carving) | Yes | Yes |
| Preview files before recovery | Yes | Yes |
| Recover files (actual write to disk) | 2 GB cap | Unlimited |
| WinPE bootable rescue media | No | Yes |
| NAS / Linux recovery | No | Yes (separate product) |
| RAID recovery | No | Yes (separate product) |
| Cloud recovery (download from cloud backups) | No | Yes |
| Full image / virtual machine recovery | No | Yes |
| Disk health (SMART) | No | Yes (separate product) |
| File repair (Office / Adobe / Archive / Video) | No | Yes (separate product) |
| Live chat / email support | No | Yes |
| Free upgrades | Limited | Lifetime |

The free-tier 2 GB recovery cap is enforced inside `DRWService.exe` /
`DRWUI.exe` (we did NOT investigate the enforcement mechanism — that
would cross the line into bypass research, which we explicitly do not do).

## Our product

**Our product is free and open-source (MIT OR Apache-2.0). There are no
feature gates. Every feature we ship works for every user.**

Specifically:

- No recovery size cap. Recover as much as your destination disk holds.
- No "Pro" / "Trial" / "Free" distinction.
- No license key. No activation server. No phone-home.
- No feature is held back for a paid tier.
- No telemetry by default. Opt-in crash reporting only, with explicit
  consent and a clear privacy policy.

## What we will NOT implement

Because we are not the reference product, we will not implement:

- Their specific license-check logic.
- Their activation server protocol.
- Any code that detects or reacts to "is this a paid user".

We will implement:

- The actual recovery features (which they gate behind a paywall) — but
  as our own original code, available to all users for free.

## Why this is fine

Reverse engineering for interoperability is a long-established practice.
The output of clean-room reverse engineering is new, original code that
happens to behave compatibly (or equivalently) with the reference product.
We are:

1. NOT redistributing any of their binaries.
2. NOT extracting their proprietary strings, icons, or assets.
3. NOT bypassing any technical protection measure.
4. NOT using any of their copyrighted material.
5. Writing 100% original Rust code that implements the same publicly
   documented algorithms (NTFS MFT walk, FAT chain recovery, file carving,
   etc.) that are described in publicly available specifications
   (Microsoft Open Specifications, Linux kernel sources, Apple technotes,
   academic papers on file carving).

If a future contributor is unsure whether a change crosses the line, the
test is simple: **"Could I have written this code having only read the
public specification, never having seen the reference product's source or
binaries?"** If yes, it's clean. If no, stop and ask.
