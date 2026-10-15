# Filesystem Parser Plugin Pattern

## Reference product (observed)

The reference product ships **one DLL per filesystem**, each exposing a
**2-function factory API**:

```c
// NTFSSearchDLL.dll
HANDLE EUCreateNTFSSearcher(...);       // full scan (metadata + carving)
HANDLE EUCreateNTFSQuickSearcher(...);  // quick scan (metadata only)

// FATSearchDLL.dll
HANDLE EUCreateFATSearcher(...);
HANDLE EUCreateFATQuickSearcher(...);

// ... same pattern for exFAT, EXT4, HFS+, Btrfs, ReFS, ZFS
```

The "RAW" searcher is different — it has 6 exports:

```c
// RAWSearchDLL.dll
HANDLE EUCreateRAWSearcher(...);
HANDLE EUCreateRAWSearcherForCellTest(...);
const AnalyserList* EUGetAnalyserList();
const AnalyserMap*  EUGetAnalyserMap();
const AnalyserMultimap* EUGetAnalyserMultimap();
const UserInfoMap*  GetRawUserInfoMap();
```

The RAW searcher has **"analysers"** — per-file-type validators that take a
candidate carved chunk and answer "is this a valid file of type X?". This is
how it avoids false positives when carving (e.g. a JPEG analyser verifies
the SOI/EOI markers and the Huffman table sanity, not just the FF D8 magic).

## Why this is a good design

1. **Plugin isolation**: a bug in the FAT parser can't crash the NTFS scan.
   Each DLL can be unloaded and reloaded.
2. **Independent versioning**: the FAT parser can be updated without
   touching the NTFS parser.
3. **Narrow API surface**: 2 functions per DLL means the contract is small
   and easy to audit.
4. **Quick vs Full split**: lets the UI offer a "fast preview" before
   committing to a full deep scan. Good UX.
5. **Analyser map for carving**: per-type validators are the right way to
   keep false-positive rates down. Naive header-only carving produces
   huge amounts of garbage.

## Our design (data-wizard-rs)

We mirror the same pattern, but in idiomatic Rust:

```rust
// crates/dw-core/src/plugin.rs

#[async_trait]
pub trait FsPlugin: Send + Sync {
    fn id(&self) -> &'static str;        // "ntfs", "fat", ...
    fn name(&self) -> &'static str;      // "NTFS 3.x parser"

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome>;
    async fn scan(
        &self,
        source: Arc<dyn DiskSource>,
        mode: ScanMode,                  // Quick or Deep
        results: Arc<ResultsStore>,
        progress: Arc<dyn ProgressSink>,
    ) -> CoreResult<()>;
}

pub enum ScanMode {
    Quick,   // metadata-only walk
    Deep,    // metadata walk + raw carving
}
```

### Plugin registration

```rust
// crates/dw-core/src/scan.rs

pub struct Scan {
    pub plugins: Vec<Arc<dyn FsPlugin>>,
    // ...
}

impl Scan {
    pub fn new_default(handle: ScanHandle, progress: Arc<dyn ProgressSink>) -> Self {
        Self {
            handle,
            plugins: vec![
                Arc::new(dw_ntfs::NtfsPlugin::default()),
                Arc::new(dw_fat::FatPlugin::default()),
                Arc::new(dw_exfat::ExfatPlugin::default()),
                Arc::new(dw_ext::ExtPlugin::default()),
                Arc::new(dw_hfs::HfsPlugin::default()),
                Arc::new(dw_btrfs::BtrfsPlugin::default()),
                Arc::new(dw_refs::RefsPlugin::default()),
                Arc::new(dw_zfs::ZfsPlugin::default()),
                // raw carving runs as part of the deep-scan pass,
                // not as a separate plugin
            ],
            progress,
        }
    }
}
```

### Analyser pattern for carving

The RAW searcher's "analyser" concept maps to per-format validators in
`dw-carver`:

```rust
// crates/dw-carver/src/structured.rs (planned)

pub trait FileAnalyser: Send + Sync {
    fn extension(&self) -> &'static str;
    fn validate(&self, data: &[u8]) -> AnalyseOutcome;
}

pub enum AnalyseOutcome {
    /// Definitely a valid file of this type, ends at `end_offset`.
    Valid { end_offset: u32 },
    /// Probably valid but can't confirm without more data.
    Maybe,
    /// Definitely not a valid file of this type.
    Invalid,
}

pub struct JpegAnalyser;
pub struct PngAnalyser;
pub struct ZipAnalyser;
pub struct PdfAnalyser;
// ... one per supported format
```

The carver then runs:

1. Scan for header magics (from `dw-signatures`).
2. For each hit, invoke the matching analyser.
3. Only emit a `RecoveredEntry` if the analyser returns `Valid` or `Maybe`.

This keeps false positives low — same approach as the reference product's
analyser map, but expressed as a trait instead of a function-pointer table.
