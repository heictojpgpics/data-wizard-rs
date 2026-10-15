# FileFilter.xml — Format Reference

> **Format only.** We document the XML schema so our UI can read/write a
> compatible filter file. We do NOT extract or redistribute the
> reference product's extension lists — our `dw-signatures` crate
> rebuilds the data from public sources.

## Schema

```xml
<xml RootName="All types" Selected="2" iconIndex="-1">
  <FileCategory ShowComment="file.jpeg"
                Description="Graphic"
                Label="common image files or digital camera photos, such as JPG, etc."
                iconIndex="0">
    <Selected>2</Selected>
    <FileType Description="TIFF Graphics file(.tif.tiff.epx.nef.arw)">
      <Extention>.tif</Extention>
      <Extention>.tiff</Extention>
      <!-- ... more extensions ... -->
      <Selected>2</Selected>
    </FileType>
    <!-- ... more FileType elements ... -->
  </FileCategory>
  <!-- ... more FileCategory elements ... -->
</xml>
```

## Elements

| Element | Attributes | Purpose |
|---|---|---|
| `xml` (root) | `RootName`, `Selected`, `iconIndex` | Top-level container. `Selected` is a tri-state: 0=unchecked, 1=checked, 2=indeterminate. |
| `FileCategory` | `ShowComment`, `Description`, `Label`, `iconIndex` | A grouping of file types (Graphic, Document, Audio, Video, Archive, etc.). |
| `FileType` | `Description` | A specific file-type group within a category. The `Description` is human-readable. |
| `Extention` | (text) | A single file extension, including the leading dot (e.g. `.jpg`). Note the misspelling "Extention" (not "Extension") — preserved for compatibility. |
| `Selected` | (text) | Tri-state: 0/1/2. Inherited from parent if absent. |

## Observed categories

| Category | Description |
|---|---|
| Graphic | Image files (JPG, PNG, TIFF, RAW camera formats, etc.) |
| Document | Office docs, PDF, text, etc. |
| Email | Outlook PST/OST, EML, etc. |
| Audio | MP3, FLAC, WAV, etc. |
| Video | MP4, AVI, MKV, etc. |
| Archive | ZIP, RAR, 7Z, etc. |
| Bookmarks | Browser bookmark files |
| Other | Catch-all |

## Our format

We will NOT use XML for our filter file. We'll use a TOML file with
the same semantic content but a cleaner schema:

```toml
# ui/file-filter.toml — our format, schema-compatible in spirit

[[category]]
name = "Graphic"
description = "Common image files and digital camera photos"
default_selected = true

  [[category.file_type]]
  name = "TIFF/RAW"
  extensions = [".tif", ".tiff", ".nef", ".arw", ".cr2", ".dng", ".heic"]
  default_selected = true

[[category]]
name = "Document"
# ...
```

This is consumed by `dw-ipc` and surfaced to the UI as a typed
`FileFilter` struct. The UI renders checkboxes; the user's selection is
passed to `start_scan` as a `Vec<String>` of extensions.

## Why we don't just read their XML

1. **Clean-room policy.** Reading their XML at runtime would couple us
   to their data layout. We rebuild from public sources instead.
2. **Localization.** Their `Description` strings are English-only in
   the XML; the actual UI localization is done via Qt `.qm` files. Our
   UI uses i18next JSON, so descriptions live in our locale files.
3. **Extensibility.** TOML is easier to extend with per-type metadata
   (MIME type, default carve analyser, etc.) than their flat XML.
