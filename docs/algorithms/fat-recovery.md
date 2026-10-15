# FAT12/16/32 recovery

> Status: implemented (`dw-fat`: bpb, direntry, fat, walker, plugin,
> timestamp re-export). Spec: Microsoft FAT32 Specification (public).

## Quick pass (metadata walk)

1. Parse the BPB (sector 0): FAT type via the spec decision tree
   (clusters < 4085 -> FAT12, < 65525 -> FAT16, else FAT32).
2. Read one FAT copy; build `FatTable`.
3. Root directory: fixed region (FAT12/16) or the `root_cluster` chain
   (FAT32).
4. Walk 32-byte entries: short 8.3 + LFN reassembly (checksum-validated
   sequence), `.`/`..` skipped, volume labels skipped.
5. Per file: follow the FAT chain -> cluster list -> canonical extents
   (see extents.md). Subdirectories recurse (cycle guard by cluster).

## Timestamps (`dw_core::fat_timestamp`)

Date u16 (year-1980/month/day bitfields) + time u16 (hour/min/sec/2) +
10ms tenths byte (creation only). Strict policy: zero = unset (None),
impossible calendar dates (Feb 30) -> None, never clamped. The tenths
refine the 2-second window and can cross the minute boundary
(14:30:20 + 1.99s). exFAT shares this decoder; timestamp u32 =
date in the high 16 bits, time in the low 16.

## Deep pass (unallocated carving)

`FatTable::free_runs(count_of_clusters)` -> runs of clusters whose raw
FAT entry is 0. Deleted files' remnants live exactly there (delete
zeroes FAT entries). The carver runs over these byte ranges ONLY -
allocated clusters are never carved (no duplicate hits on live files).

## Deleted entries

0xE5 first byte; first cluster + size survive in the entry; chain gives
`[first]` -> partial extents -> `PartiallyRecovered` unless the carver
later validates a header at that location.
