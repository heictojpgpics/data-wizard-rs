# File Carving

> Algorithm spec for `crates/dw-carver/`. Derived from published digital-
> forensics literature (Richard & Roussev "Forensic File Carving" 2006;
- NIST CFRED; Foremost/Scalpel/Photorec docs). NOT from the reference
> product's binaries.

## When carving is invoked

Carving runs as part of the **Deep scan** phase, after the metadata walk
is done. The carver operates on regions of the source that the filesystem
parser has marked as "free" (NTFS $Bitmap clear, FAT 0x000, exFAT
allocation bitmap clear, etc.).

## Algorithms

### 1. Header / footer carving (default)

The simplest algorithm. For each signature in the database:

1. Scan the source for the header magic at the expected offset.
2. From the header position, scan forward for the footer magic.
3. The candidate file spans `[header_pos, footer_pos + footer_len)`.
4. Validate via the per-format analyser.
5. If valid, emit a `RecoveredEntry`.

**Pros**: fast, simple, works for most contiguous files.
**Cons**: fails for fragmented files; produces false positives if the
header magic is common (e.g. `FF D8` for JPEG appears in many binary
files).

### 2. Bifragment gap carving

For files that have exactly **two** fragments separated by a small
overwriting gap (typical when a small file is deleted and a different
small file overwrites part of it):

1. Find the header.
2. Find the footer.
3. For every plausible split point `s` in `[header, footer]`:
   - Reconstruct the candidate as `data[header..s] + data[gap_end..footer]`
     where `gap_end = s + gap_size`.
   - Validate via the analyser.
   - If valid, emit and stop.

**Complexity**: O(N * gap_size) per candidate where N is the file size.
Only invoked when (a) header/footer carving failed, and (b) the user
opted into "deep" mode.

### 3. Structured carving

For file types with rich internal structure, we walk that structure to
compute the actual end of the file rather than trusting the footer scan.

- **ZIP**: scan for the End-of-Central-Directory record (EOCD), then walk
  the central directory backward to find the first local file header.
- **RAR**: scan for the archive end header, walk backward.
- **PDF**: scan for `%%EOF`, then walk the xref table backward.
- **EXT4**: if we find an ext4 superblock, we have a full filesystem
  image (not a single file) — invoke the ext4 plugin recursively.
- **PNG**: walk IEND chunk.
- **JPEG**: scan for FF D9 (EOI), validate the preceding marker stream.

Structured carving is the most accurate but the most expensive. We run
it only on file types where it's defined.

### 4. Fragment-map carving (planned, not in v0.1)

For files with many small fragments (typical of large video files on a
heavily fragmented disk). Maintains a per-signature "fragment candidate"
heap and searches for an assignment that satisfies the file's internal
ordering constraints (e.g. for a JPEG, each fragment must start with a
valid marker).

Expensive — O(N^2) in the number of fragments. Only invoked when
bifragment carving fails AND the user opted into "deep" mode AND the file
type has a known internal ordering.

## Per-format analysers

The carver invokes a per-format analyser to validate candidate files.
The analyser returns:

```rust
pub enum AnalyseOutcome {
    /// Definitely a valid file of this type, ends at `end_offset`.
    Valid { end_offset: u32 },
    /// Probably valid but can't confirm without more data.
    Maybe,
    /// Definitely not a valid file of this type.
    Invalid,
}
```

Planned analysers (one per supported format):

| Format | Analyser checks |
|---|---|
| JPEG | SOI (FF D8) ... markers ... EOI (FF D9). Validates Huffman table, restart markers, JFIF/EXIF header. |
| PNG | 8-byte signature ... IHDR ... IDAT (>=1) ... IEND. Validates CRC of each chunk. |
| GIF | GIF87a/GIF89a ... image descriptors ... trailer (0x3B). |
| BMP | BM ... DIB header ... pixel data. |
| TIFF | II*\0 or MM*\0 ... IFD chain. |
| ZIP | Local file header ... central directory ... EOCD. |
| RAR | Rar!\x1a\x07 ... archive header ... end header. |
| 7z | 7z\xbc\xaf\x27\x1c ... header. |
| PDF | %PDF-1.x ... xref ... %%EOF. |
| MP4 | ftyp box ... mdat ... moov. |
| MP3 | ID3v2 ... frames ... or frame sync 0xFFE. |
| WAV | RIFF ... WAVE ... fmt ... data. |
| FLAC | fLaC ... metadata blocks ... audio frames. |
| OGG | OggS ... pages. |
| DOCX/XLSX/PPTX | (ZIP-based — use ZIP analyser then validate internal structure) |
| DOC/XLS/PPT (legacy) | OLE2 compound document magic D0 CF 11 E0 ... |
| SQLite | 5-byte "SQLite format 3\x00" header ... page size ... |
| ELF | \x7fELF ... |
| PE | MZ ... PE\0\0 ... |

Each analyser is a small Rust struct (~50–200 LoC) implementing the
`FileAnalyser` trait.

## Performance budget

For a 1 TB disk with 100 GB of free space:

- Header/footer scan: ~5 minutes (depends on signature DB size and
  disk throughput).
- Analyser validation: ~30 seconds (only runs on header hits, which are
  typically 10K-100K).
- Total: ~6 minutes. Within the same order of magnitude as the reference
  product.

## Parallelism

The carver parallelises across:

- **Source windows**: the disk is split into N chunks (one per core),
  each chunk scanned independently.
- **Analysers**: within a chunk, candidate validation is parallel.

The `ResultsStore` is the synchronisation point — it's `Arc<ResultsStore>`
with internal locking via `parking_lot::RwLock`.


---

## Implemented design (dw-carver, 2026-10)

### Signature database (258 formats)

`dw-signatures`: three TOML files compiled into the binary at build time
(no runtime file dependency). Sources, cited per entry: GCF (Kessler),
Wikipedia List of file signatures (fetched + parsed from the live table),
NIST DFXML, libmagic. Wildcard bytes (`??`) supported in all patterns.

### Per-format structure analysers (`analysers.rs`)

Written from public format specifications, each with unit tests:

- **JPEG** (T.81): marker walk from SOI; every segment length validated;
  on SOS, the entropy scan takes the FIRST `FF D9` - entropy data cannot
  contain a raw FF D9 (FF is escaped as FF 00 or is a marker).
- **PNG** (ISO 15948): chunk walk; first chunk must be IHDR; IEND must
  have length 0; ends after the IEND CRC.
- **ZIP** (PKWARE APPNOTE): first EOCD (`50 4B 05 06`); the comment
  length field extends the end.

### Four-tier resolution (per candidate)

| Tier | Condition | Result |
|---|---|---|
| 1 | analyser validates the structure to the true end | `CarvedValidated`, exact extents, directly recoverable |
| 2 | structure plausible but end beyond the analysis cap (64 MiB or the signature's max_size) | `Carved`, extents capped - honest partial |
| 3 | footer signature found forward-scan (GIF `3B`, PDF `%%EOF`, ...; wildcard patterns, window overlap) | `Carved`, extents header..footer |
| 4 | no analyser, no footer | size-0 marker entry, halved confidence; recovery REFUSES it (typed error) - never writes guessed bytes |

Structurally INVALID candidates emit NOTHING - that is the false-positive
filter. Candidates at the same offset are deduplicated (highest weight
wins, alphabetical tiebreak; a ZIP archive matches zip/jar/apk
signatures simultaneously).

### Where carving runs

- FAT deep scan: `FatTable::free_runs` (raw FAT entry == 0).
- exFAT deep scan: `ExfatBitmap::free_runs` (allocation bitmap).
- NTFS deep scan: currently whole-volume (refining to $Bitmap-guided is
  planned work).
- Unknown filesystem: the app layer runs the whole-source fallback for
  deep scans (RAWSearchDLL's role).
