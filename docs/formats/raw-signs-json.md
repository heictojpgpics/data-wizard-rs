# raw_signs.json — Format Reference

> **Format only.** We document the JSON schema so our `dw-signatures`
> crate can read/write a compatible signature DB. We do NOT extract or
> redistribute the reference product's signature entries — our DB is
> rebuilt from public sources (Gary Kessler GCF, NIST DFXML, PRONOM,
> libmagic).

## Schema

```json
{
  "<extension>": {
    "header": {
      "signs": [
        "<offset>,<hex_bytes>,<weight>"
      ]
    },
    "tail": {
      "signs": [
        "<offset_from_end>,<hex_bytes>,<weight>"
      ]
    },
    "min": <min_file_size_bytes>,
    "max": <max_file_size_bytes>
  },
  ...
}
```

## Fields

| Field | Type | Meaning |
|---|---|---|
| `<extension>` (top-level key) | string | File extension without leading dot, lowercase. E.g. `"pdf"`, `"jpg"`. |
| `header.signs[]` | array of strings | Header magic signatures. Each string is `"offset,hex_bytes,weight"`. |
| `tail.signs[]` | array of strings | Tail (footer) magic signatures. Each string is `"offset_from_end,hex_bytes,weight"`. |
| `min` | integer | Minimum file size in bytes. Files smaller than this are rejected. |
| `max` | integer | Maximum file size in bytes. 0 or omitted = unlimited. |

## Signature string format

Each signature string is comma-separated:

```
<offset>,<hex_bytes>,<weight>
```

| Component | Type | Meaning |
|---|---|---|
| `offset` | integer | For header signatures: byte offset from start of file. For tail signatures: byte offset from END of file (so 0 = last byte, 1 = second-to-last, etc.). |
| `hex_bytes` | hex string | The magic bytes to match at that offset. Even number of hex digits. |
| `weight` | float | Confidence weight in range [0, 1]. Higher = more confident. Used to disambiguate when multiple signatures match. |

## Example (synthetic, not from any real product)

```json
{
  "pdf": {
    "header": {
      "signs": ["0,255044462D,0.99"]
    },
    "tail": {
      "signs": ["0,2525454F46,0.99"]
    },
    "min": 64,
    "max": 0
  }
}
```

This means: a PDF file starts with the bytes `25 50 44 46 2D` (which is
`%PDF-`) at offset 0, ends with `25 25 45 4F 46` (which is `%%EOF`) at
offset 0 from end, must be at least 64 bytes, and has no max size.

## Our format

We will NOT use this exact JSON format. Our `dw-signatures` crate uses
a binary format (`signatures.bin`) documented in
[`signatures-bin.md`](signatures-bin.md). The source format we author
in is TOML:

```toml
# crates/dw-signatures/assets/public-signatures/pdf.toml

[[signature]]
extension = "pdf"
mime = "application/pdf"
header_offset = 0
header_magic = "25 50 44 46 2D"  # %PDF-
tail_offset_from_end = 0
tail_magic = "25 25 45 4F 46"    # %%EOF
min_size = 64
max_size = 0
weight = 990  # 0..=1000 (we use integers, not floats)
```

The build script (`crates/dw-signatures/build.rs`) compiles all TOML
files under `assets/public-signatures/` into a single `signatures.bin`
embedded in the binary at build time.

## Why we don't just read their JSON

1. **Clean-room policy.** Their signature data is proprietary. We
   rebuild from public sources.
2. **Provenance.** Each of our signatures has a citation to the public
   source it came from (GCF row number, PRONOM ID, etc.).
3. **Type safety.** Our TOML schema is parsed into a typed `Signature`
   struct; their JSON is parsed into a `Vec<Map<String, Value>>` with
   no schema enforcement.
4. **Build-time embed.** We compile the DB into the binary at build
   time — no runtime file read, no chance of the DB going missing.
