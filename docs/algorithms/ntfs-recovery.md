# NTFS Recovery Algorithm

> This document describes the algorithm we **will** implement in
> `crates/dw-ntfs/`. The algorithm is derived from **public NTFS
> documentation only**: Microsoft Open Specifications MS-NTFS, the Linux
> kernel `fs/ntfs3/` sources, libntfs-3g, and Wikipedia. None of this is
> taken from the reference product's binaries.

## NTFS on-disk layout (public spec)

```
+----------------------+ offset 0
| VBR (Volume Boot     |
|  Record)             |  512 B (1 sector)
|  - OEM: "NTFS    "   |
|  - BPB               |
|  - MFT offset        |
|  - MFT record size   |
+----------------------+
| ... (NTFS metadata)  |
+----------------------+ MFT offset (typically ~1 MiB)
| MFT (Master File     |
|  Table)              |
|  - $MFT (file 0)     |  self-referencing
|  - $MFTMirr (file 1) |
|  - $LogFile (file 2) |
|  - $Volume (file 3)  |
|  - $AttrDef (file 4) |
|  - root dir (file 5) |
|  - $Bitmap (file 6)  |  MFT-record-usage bitmap
|  - $Boot (file 7)    |
|  - $BadClus (file 8) |
|  - $Secure (file 9)  |
|  - $UpCase (file 10) |
|  - $Extend (file 11) |
+----------------------+
| ... user files ...   |
+----------------------+
```

Each MFT record is typically 1024 bytes. Each record describes one file
(or directory) as a list of attributes:

- `$STANDARD_INFORMATION` (0x10): timestamps, flags
- `$FILE_NAME` (0x30): name, parent directory reference
- `$DATA` (0x80): the file's contents (resident or non-resident)
- `$INDEX_ROOT` / `$INDEX_ALLOCATION` (0x90 / 0xA0): directory contents
- `$ATTRIBUTE_LIST` (0x20): points to additional MFT records if attributes
  don't fit in one
- `$OBJECT_ID` (0x40): 16-byte GUID for link tracking

## Probe

1. Read sector 0 (the VBR).
2. Verify bytes 3..11 == `b"NTFS    "`.
3. Parse the BPB to extract:
   - `bytes_per_sector` (typically 512)
   - `sectors_per_cluster` (1, 2, 4, 8, 16, 32, 64, or 128)
   - `mft_logical_cluster_number` (offset of MFT in clusters)
   - `mft_record_size` (encoded as a signed power of 2)
4. If all checks pass, return `ProbeOutcome::Yes(FsProbe { ... })`.
   Otherwise return `ProbeOutcome::No`.

## Quick scan (metadata walk)

1. Compute the byte offset of the MFT:
   `mft_offset = mft_logical_cluster_number * sectors_per_cluster * bytes_per_sector`
2. Read the first MFT record (record 0 = $MFT itself). Parse its
   `$DATA` attribute to get the full MFT's cluster chain (the MFT is
   itself fragmented and described by its own $DATA attribute).
3. Walk every MFT record from 0 to the highest record number in $MFT's
   `$DATA`:
   1. Read the 1024-byte record.
   2. Verify the FILE magic (`b"FILE"`).
   3. Check the flags field: bit 0 = in use, bit 1 = directory.
   4. If the record is NOT in use (bit 0 clear), this is a deleted file
      candidate. The record's $FILE_NAME and $DATA attributes may still
      be intact (NTFS doesn't zero the record on delete; it just clears
      the in-use bit and marks the clusters free in $Bitmap).
   5. Walk the attribute list:
      - For each `$FILE_NAME`: capture the name and parent directory
        reference (file reference = record number | sequence number).
      - For each `$DATA`:
        - If resident: data is inline in the attribute.
        - If non-resident: parse the run list (compressed encoding of
          `(length, offset)` runs).
      - For each `$INDEX_ROOT` / `$INDEX_ALLOCATION`: this is a directory;
        recurse into its children.
   6. Resolve the parent directory reference to a path. If the parent
      was deleted before this file, the parent reference may dangle —
      we surface these as `[Lost Files]/<name>`.
4. Cross-reference with $UsnJrnl:\$J (USN journal) for files deleted
   very recently (their MFT record may already be reused).

## Deep scan (carving)

For every cluster NOT in $Bitmap (i.e. free clusters):

1. Read a window (e.g. 1 MiB) of contiguous free clusters.
2. Run the [`Matcher`](../../crates/dw-signatures/src/matcher.rs) over
   the window.
3. For each header hit:
   - Invoke the per-format analyser (e.g. JPEG analyser verifies SOI,
     DQT, DHT, SOF, EOI markers).
   - If valid, emit a `RecoveredEntry` with `was_deleted=true`,
     `status=Carved`, `start_lba=None` (carved files don't have FS-native
     cluster info).

## Recovery (write to disk)

When the user picks files to recover:

1. For each entry, look up the source:
   - If `status == Intact`: read the cluster chain from $DATA and copy
     to destination.
   - If `status == PartiallyRecovered`: read whatever fragments we have,
     zero-fill gaps.
   - If `status == Carved`: read from the carve offset for the carver's
     computed length.
2. Write to the destination. **Never write to the source disk** (would
   overwrite the very clusters we're trying to recover).
3. Verify with the file's structural checksum (JPEG EOI, ZIP EOCD, etc.)
   if an analyser is available.

## MFT record parsing — zero-copy

We parse MFT records as `bytemuck::Pod` views into the mmap'd MFT region.
No per-field allocation on the hot path. This lets us walk a 4 MiB MFT
(millions of records) in well under a second on a modern SSD.

## Tests (planned)

- Unit tests with hand-crafted MFT records (resident + non-resident $DATA).
- Integration tests with a real NTFS image (created with `mkntfs` under
  WSL on the CI runner).
- Fuzz tests with `cargo fuzz` on the MFT record parser.

## References

- Microsoft Open Specifications MS-NTFS: `[MS-NTFS]` PDF
- Linux kernel: `fs/ntfs3/` (read-only reference implementation)
- libntfs-3g: <https://github.com/tuxera/ntfs-3g>
- Wikipedia: NTFS
