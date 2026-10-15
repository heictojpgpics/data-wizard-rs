# Canonical physical extents

> Status: implemented (`dw-core/src/results.rs`, `dw-core/src/extents.rs`).
> Applies to every engine: FAT, exFAT, NTFS, and the carver.

## The model

Every `RecoveredEntry` carries `extents: Vec<Extent>` where
`Extent = { start_byte, length }` are ABSOLUTE byte offsets into the scan
source (image file or raw device), listed in file order. The extents are
the single source of truth for recovery I/O:

- `dw_core::recovery::recover_entry_to_path` streams the extents in order
  (8 MiB chunks, flush + fsync before success). Fragmented files work
  natively: a FAT chain 4 -> 5 and an NTFS run list with two runs both
  become multi-extent lists and are written identically.
- `start_lba` is DERIVED (`recompute_start_lba()` = first extent / 512).
  FS cluster numbers must never leak into it.

## Construction rule (all engines)

`dw_core::extents::merge_and_trim_offsets(offsets, step, file_size)`:

1. the walker supplies one byte offset per FS allocation unit, ascending,
   via `cluster_to_byte` (FAT/exFAT) or `lcn * bytes_per_cluster` (NTFS);
2. consecutive offsets merge into one extent;
3. the total is trimmed to the file's declared size (the last unit is cut);
4. a chain shorter than the declared size yields proportionally shorter
   extents and the entry is marked `PartiallyRecovered` - honest partial,
   never padded.

## Priority order in the recovery writer

1. `resident_data` (NTFS resident $DATA) - exact inline bytes, no I/O;
2. `extents` - streamed;
3. size == 0 - empty file;
4. otherwise - typed `RecoveryError::NoKnownLocation`. We refuse to guess
   rather than write wrong bytes.

## Deleted-file semantics

- FAT: deletion zeroes the FAT entries; the directory entry (first
  cluster + size) survives -> chain yields `[first]` only -> partial
  extents (unless the carver validates the file's structure later).
- exFAT NoFatChain: the directory entry keeps `first_cluster` AND
  `data_length` AND `NoFatChain`, so deleted files remain FULLY
  recoverable from metadata alone.
- NTFS: the $DATA run list usually survives deletion -> full extents.
