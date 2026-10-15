# exFAT recovery

> Status: implemented (`dw-exfat`: boot, direntry, fat, bitmap, walker,
> plugin). Spec: exFAT v1.00 (public).

## On-disk structures this engine parses

| Structure | Layout source |
|---|---|
| Boot sector | spec table (3.1): offsets 64..120 |
| FAT (u32 entries) | spec 6.2: 0=free, >=0xFFFFFFF8 EOC, 0xFFFFFF7 bad |
| Allocation bitmap dir entry | spec 7.1: flags@1, first_cluster@5..9, 19-byte length@9..28 (high 11 bytes must be zero) |
| File dir entry (0x85/0x05) | spec 7.3: SecondaryCount@1, attrs@4..6, 3x u32 timestamps@8..20, 10ms@20, 3 UTC offsets@21..24 |
| Stream extension (0xC0/0x40) | spec 7.4 (cross-checked vs Linux exfat `struct exfat_dentry_stream`): flags@1, Reserved1@2, NameLength@3, NameHash@4..6, ValidDataLength@8..16, FirstCluster@20..24, DataLength@24..32 |
| File name (0xC1/0x41) | 15 UTF-16 chars @2..32 |

## Quick pass

Root directory chain from `root_dir_first_cluster` (always FAT-chained);
entry-set state machine (primary + secondary_count secondaries: exactly
one stream + N name entries; malformed sets are skipped wholesale);
recursion into directories whose content is readable (in-use, or deleted
with NoFatChain); cycle guard.

## Deleted policy (the interesting part)

Deleting clears InUse on the whole set and zeroes FAT entries, but the
entry bytes survive:
- **NoFatChain deleted**: first_cluster + data_length + NoFatChain all
  survive -> FULL extents still computable -> `Intact`, fully recoverable.
- **FAT-chained deleted**: chain yields `[first]` -> `PartiallyRecovered`.

## Deep pass

The walk captures the Allocation Bitmap directory entry; the plugin then
reads the bitmap chain, `ExfatBitmap` computes free runs (word-at-a-time
for uniform regions), and the carver runs over exactly those byte ranges.
Bit order: LSB-first (cluster 2 = bit 0 of byte 0), per the Linux driver.

## Timestamps

Same FAT-family decoder as FAT (shared `dw_core::fat_timestamp`);
u32 = date<<16 | time; CreateTime10ms refines creation like FAT tenths.
