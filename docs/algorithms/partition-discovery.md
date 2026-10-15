# Partition discovery (MBR / EBR / GPT)

> Status: implemented (`dw-disk/src/partition.rs`).
> Specs: MS-DOS partition scheme (public); UEFI Specification ch. 5.

## MBR

LBA0: signature `55 AA` @510; four 16-byte entries @446
(status, CHS start, type, CHS end, LBA start u32 LE, sectors u32 LE).
No signature -> `DiscoveredTable::None` (raw volume - still scannable).

Type 0xEE in any slot -> GPT protective -> parse GPT at LBA1.

## EBR chains (extended partitions, types 0x05/0x0F/0x85)

Each EBR holds entry[0] = THIS logical partition (start RELATIVE to the
EBR's own LBA) and entry[1] = the NEXT EBR (start relative to the extended
partition's base LBA). The walk is bounded (128 links) and cycle-safe
(visited set). Unreadable/unsigned EBRs terminate the chain gracefully.

## GPT (CRC-validated, strict)

Header at LBA1: `EFI PART`, revision 1.x, HeaderSize == 92,
MyLBA == 1, header CRC32 over bytes 0..92 with the CRC field zeroed,
entry-array CRC32 over `num_entries * entry_size` bytes.
ANY mismatch -> typed error; we never fall back to a table that fails
its own checksum. Entry count capped at 128, array capped at 64 KiB.
Entries with all-zero type GUIDs are skipped; first/last LBA inverted or
zero -> skipped (corrupt single entry never fails the table).

Known type GUIDs map to names (EFI System, Basic data, Linux filesystem,
MSR, Windows recovery, Linux RAID/LVM/swap).

## App integration

- `discover_partitions(read_at) -> DiscoveredTable` (rich: scheme, type
  name, GPT label) -> `.into_geometry()` (canonical fs hints).
- `PartitionView { inner, offset, size }`: offset-translating,
  bounds-checked source. `start_scan(partition: Some(index))` wraps the
  source in a view - filesystem parsers see exactly the partition as a
  standalone volume, no copies.
- `list_partitions(source_uri)`: discovery + per-partition FS probing
  (first plugin that claims the view wins).
- Windows `list_disks`: PhysicalDrive0..63 enumeration + per-drive
  discovery; volumes (drive letters) kept as directly-scannable entries.

Validated on the msvc target via cross-check (0 errors); runtime
verification runs in the Nightly Verification workflow (this repo).
