# signatures.bin — Our Format (v1)

The compiled file-signature database produced by
`crates/dw-signatures/build.rs` from the public-source TOML files in
`crates/dw-signatures/assets/public-signatures/`.

## Why a binary format

- **Fast load.** mmap'd at startup, no JSON/TOML parse cost.
- **Small.** ~50 KB for ~1000 signatures (vs. ~500 KB JSON).
- **Type-safe.** Parsed with `bytemuck::Pod` zero-copy views.
- **Build-time embedded.** Included via `include_bytes!` so there's no
  runtime file dependency.

## Header (32 bytes)

```text
Offset  Size  Field
  0     4     magic       "DWSG" (Data-Wizard SiGnatures)
  4     2     version     u16 LE = 1
  6     2     flags       u16 LE (bit 0 = has-tail)
  8     4     count       u32 LE
 12    18     reserved    zero
 30     2     header_crc  u16 LE (CRC-16 of bytes 0..30)
```

Wait — that's only 32 bytes if we use a 2-byte CRC at the end. Let me
re-state the layout cleanly:

```text
Offset  Size  Field
  0     4     magic       "DWSG"
  4     2     version     u16 LE = 1
  6     2     flags       u16 LE (bit 0 = has-tail)
  8     4     count       u32 LE
 12    18     reserved    zero
 30     2     header_crc  u16 LE
```

Total: 32 bytes.

## Records (variable length, count * N)

Each record is laid out as:

```text
Offset  Size  Field
  0     1     ext_len     u8
  1     ext_len  ext      ASCII, no NUL terminator
  +1     1     mime_len   u8 (0 = no MIME)
  +1     mime_len  mime   ASCII
  +1     4     hdr_off    u32 LE   header offset
  +1     2     hdr_len    u16 LE
  +1     hdr_len  hdr_magic
  +1     4     tail_off   u32 LE   tail offset from END (0 = no tail)
  +1     2     tail_len   u16 LE
  +1     tail_len  tail_magic (omitted if tail_off = 0)
  +1     8     min_size   u64 LE
  +1     8     max_size   u64 LE (0 = unlimited)
  +1     2     weight     u16 LE  0..=1000 (confidence in per-mille)
```

All multi-byte integers are little-endian. Strings are ASCII (no UTF-8
— file extensions and MIME types are ASCII in practice).

## CRC

The header CRC is CRC-16/CCITT-FALSE over bytes 0..30. Used to detect
truncation/corruption of the header. Record-level CRCs are NOT stored
(the whole file is small enough to re-validate on load if needed).

## Versioning

- Version 1 (current): as described above.
- Version 2 (future): will add per-signature analyser ID (so the
  carver knows which `FileAnalyser` to invoke without matching on
  extension).

Version bumps are non-breaking: the loader checks `version` and
rejects versions it doesn't understand.

## Build pipeline

```
crates/dw-signatures/assets/public-signatures/*.toml
        |
        |  (build.rs reads all *.toml, validates, sorts by extension)
        v
crates/dw-signatures/assets/signatures.bin  (committed)
        |
        |  (include_bytes! at compile time)
        v
crates/dw-signatures/src/db.rs::SignatureDb  (parsed once, mmap'd)
        |
        |  (Matcher::find_headers() scans a byte window)
        v
crates/dw-carver/src/structured.rs  (per-format analyser validates)
```

## Public sources we use to populate the TOML files

| Source | URL | License |
|---|---|---|
| Gary Kessler GCF File Signatures Table | <https://www.garykessler.net/software/filemountains.txt> | Public (Gary Kessler) |
| NIST DFXML | <https://github.com/dfxml-working-group/dfxml_schema> | Public domain (NIST) |
| PRONOM | <https://www.nationalarchives.gov.uk/PRONOM/> | UK Open Government License |
| TrID | <https://github.com/0xC0DEFEED/trid> (free subset) | GPL |
| libmagic | <https://github.com/file/file> | BSD |
| Wikipedia "List of file signatures" | <https://en.wikipedia.org/wiki/List_of_file_signatures> | CC-BY-SA |

Each TOML entry has a `source = "..."` field citing where the magic
came from. This is for provenance — if a signature turns out to be
wrong, we can trace it back to the source.
