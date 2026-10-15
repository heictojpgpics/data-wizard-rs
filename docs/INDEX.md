# Reverse-Engineering Documentation Index

This directory contains the **clean-room** reverse-engineering documentation for the
reference product whose publicly observable architecture we studied as input to
the design of `data-wizard-rs`.

## What we DID do

- Examined the publicly distributed installer files (NSIS `.exe`, Apple `.dmg`,
  and a `.zip` of the installed payload).
- Catalogued **observable structural facts**: file inventory, PE export names
  (public API surface), import DLL names, sections, security flags
  (ASLR/DEP/CFG), version-info strings.
- Documented the **modular per-filesystem parser design** (one `*SearchDLL.dll`
  per filesystem with a 2-export factory API).
- Documented the **process model** (launcher + UI + service + report generator).
- Documented the **public configuration file formats** (`FileFilter.xml`,
  `raw_signs.json`, `PEtools.xml`) — formats only, not the proprietary data.
- Documented the **WinPE rescue environment** structure (custom kernel drivers,
  boot configuration).

## What we did NOT do

- We did NOT decompile to extract verbatim source code for reuse.
- We did NOT extract their proprietary strings, icons, images, or UI assets.
- We did NOT extract their proprietary file-signature database. Our
  `dw-signatures` crate rebuilds the database from public sources (Gary
  Kessler's GCF file signatures, NIST DFXML, PRONOM, libmagic).
- We did NOT bypass, patch, or emulate any license/activation check.
- We did NOT redistribute any third-party binaries.

## Documentation map

| Document | Purpose |
|---|---|
| [`architecture/overview.md`](architecture/overview.md) | High-level architecture: process model, module map, IPC topology |
| [`architecture/process-model.md`](architecture/process-model.md) | The 4-process model (launcher, UI, service, report) and why we collapse to 1 + helper |
| [`architecture/fs-parser-pattern.md`](architecture/fs-parser-pattern.md) | The per-filesystem plugin pattern (the cleanest idea we borrowed) |
| [`architecture/licensing-boundary.md`](architecture/licensing-boundary.md) | What's paywalled in the reference product (publicly visible only) and how our OSS version handles it |
| [`algorithms/scan-phases.md`](algorithms/scan-phases.md) | Probe -> Quick -> Deep -> Finalize lifecycle |
| [`algorithms/ntfs-recovery.md`](algorithms/ntfs-recovery.md) | NTFS MFT walk + USN journal + carving |
| [`algorithms/fat-recovery.md`](algorithms/fat-recovery.md) | FAT12/16/32 directory walk + FAT chain recovery |
| [`algorithms/exfat-recovery.md`](algorithms/exfat-recovery.md) | exFAT bitmap + stream extensions |
| [`algorithms/ext-recovery.md`](algorithms/ext-recovery.md) | ext2/3/4 inode table + extents + journal |
| [`algorithms/carving.md`](algorithms/carving.md) | Header/footer, bifragment gap, structured carving |
| [`formats/filefilter-xml.md`](formats/filefilter-xml.md) | FileFilter.xml schema (format only — our data is rebuilt) |
| [`formats/raw-signs-json.md`](formats/raw-signs-json.md) | raw_signs.json schema (format only — our data is rebuilt) |
| [`formats/petools-xml.md`](formats/petools-xml.md) | PEtools.xml schema (WinPE boot setup) |
| [`formats/signatures-bin.md`](formats/signatures-bin.md) | Our `signatures.bin` v1 format |
| [`ui-ux/flows.md`](ui-ux/flows.md) | User flows: select source -> scan -> review -> recover |
| [`ui-ux/patterns.md`](ui-ux/patterns.md) | UI patterns (NOT pixel clones — terminology and information architecture only) |
| [`security/findings.md`](security/findings.md) | Security posture of reference product (ASLR/CFG gaps) — things we will NOT replicate |
| [`security/our-posture.md`](security/our-posture.md) | Our security posture: full ASLR/DEP/CFG, memory-safe core, no telemetry-by-default |
| [`tech-stack/reference.md`](tech-stack/reference.md) | Reference product tech stack (observable from DLL presence) |
| [`tech-stack/ours.md`](tech-stack/ours.md) | Our tech stack and rationale |
| [`glossary.md`](glossary.md) | Glossary of terms |
| [`../analysis/reports/inventory.md`](../analysis/reports/inventory.md) | Raw PE inventory (535 binaries catalogued) |
| [`../analysis/reports/core_api_surface.md`](../analysis/reports/core_api_surface.md) | Public API surface of the core modules |
