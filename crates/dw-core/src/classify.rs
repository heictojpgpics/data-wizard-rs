//! File classification: extension -> category, and category display.
//!
//! The categories mirror the reference product's filter groups (photos,
//! videos, audio, documents, archives, email, code) so scan results can
//! be browsed the way users expect from recovery tools. Classification
//! is display-grade, not forensic: a `.jpg` inside a carved blob is a
//! photo regardless of where it came from.

/// Broad result category used for filtering and stats.
#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize,
)]
#[allow(non_camel_case_types)]
pub enum Category {
    photos,
    videos,
    audio,
    documents,
    archives,
    email,
    code,
    other,
}

impl Category {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Category::photos => "Photos",
            Category::videos => "Videos",
            Category::audio => "Audio",
            Category::documents => "Documents",
            Category::archives => "Archives",
            Category::email => "Email",
            Category::code => "Code",
            Category::other => "Other",
        }
    }

    /// Classify from a lowercase extension (no dot).
    #[must_use]
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            // Photos (raster + raw + vectors).
            "jpg" | "jpeg" | "jpe" | "jfif" | "png" | "gif" | "bmp" | "tif" | "tiff" | "webp"
            | "heic" | "heif" | "avif" | "ico" | "psd" | "svg" | "dng" | "cr2" | "cr3" | "nef"
            | "arw" | "orf" | "rw2" | "raf" | "raw" => Category::photos,

            // Videos.
            "mp4" | "m4v" | "mov" | "avi" | "mkv" | "webm" | "wmv" | "flv" | "f4v" | "mpg"
            | "mpeg" | "mp2" | "3gp" | "3g2" | "mts" | "m2ts" | "mxf" | "vob" | "ogv" => {
                Category::videos
            }

            // Audio.
            "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "oga" | "opus" | "wma" | "aiff"
            | "aif" | "mid" | "midi" | "amr" => Category::audio,

            // Documents.
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
            | "rtf" | "txt" | "md" | "csv" | "tsv" | "epub" | "pages" | "numbers" | "key"
            | "hwp" => Category::documents,

            // Archives.
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "lz4" | "tgz" | "tbz2"
            | "txz" | "cab" | "iso" | "dmg" | "vhd" | "vhdx" | "wim" | "img" | "dd" | "vmdk" => {
                Category::archives
            }

            // Email stores.
            "pst" | "ost" | "eml" | "msg" | "mbox" | "dbx" => Category::email,

            // Code / text-source (recovery-relevant for developers).
            "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "py" | "js" | "jsx" | "ts" | "tsx"
            | "java" | "kt" | "go" | "rb" | "php" | "cs" | "swift" | "m" | "mm" | "sh" | "ps1"
            | "bat" | "cmd" | "html" | "htm" | "css" | "scss" | "json" | "xml" | "yml" | "yaml"
            | "toml" | "ini" | "cfg" | "conf" | "sql" | "ipynb" => Category::code,

            _ => Category::other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_extensions_map_correctly() {
        assert_eq!(Category::from_extension("jpg"), Category::photos);
        assert_eq!(Category::from_extension("cr3"), Category::photos);
        assert_eq!(Category::from_extension("mp4"), Category::videos);
        assert_eq!(Category::from_extension("m2ts"), Category::videos);
        assert_eq!(Category::from_extension("flac"), Category::audio);
        assert_eq!(Category::from_extension("docx"), Category::documents);
        assert_eq!(Category::from_extension("vhdx"), Category::archives);
        assert_eq!(Category::from_extension("pst"), Category::email);
        assert_eq!(Category::from_extension("rs"), Category::code);
        assert_eq!(Category::from_extension("exe"), Category::other);
        assert_eq!(Category::from_extension(""), Category::other);
    }

    #[test]
    fn labels_are_stable_display_strings() {
        assert_eq!(Category::photos.label(), "Photos");
        assert_eq!(Category::other.label(), "Other");
    }
}
// ---------------------------------------------------------------------------
// Scan statistics (category + status aggregations)
// ---------------------------------------------------------------------------

/// Aggregated statistics over a scan's results, computed with bounded
/// memory (the store is walked in pages, never fully snapshotted).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanStats {
    pub total_files: u64,
    pub total_bytes: u64,
    /// Per status: count + bytes.
    pub by_status: Vec<(String, u64, u64)>,
    /// Per category: count + bytes.
    pub by_category: Vec<(Category, u64, u64)>,
}

/// Compute stats over `results` by paging through it in batches.
#[must_use]
pub fn compute_stats(results: &crate::results::ResultsStore) -> ScanStats {
    use std::collections::BTreeMap;

    let mut stats = ScanStats::default();
    let mut statuses: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut categories: BTreeMap<Category, (u64, u64)> = BTreeMap::new();

    const PAGE: usize = 4096;
    let mut offset = 0usize;
    loop {
        let page = results.page(offset, PAGE);
        if page.is_empty() {
            break;
        }
        for e in &page {
            stats.total_files += 1;
            stats.total_bytes += e.size;

            let s = statuses.entry(status_key(&e.status)).or_insert((0, 0));
            s.0 += 1;
            s.1 += e.size;

            let cat = Category::from_extension(&e.extension);
            let c = categories.entry(cat).or_insert((0, 0));
            c.0 += 1;
            c.1 += e.size;
        }
        offset += page.len();
        if page.len() < PAGE {
            break;
        }
    }

    stats.by_status = statuses.into_iter().map(|(k, (n, b))| (k, n, b)).collect();
    stats.by_category = categories
        .into_iter()
        .map(|(k, (n, b))| (k, n, b))
        .collect();
    stats
}

fn status_key(s: &crate::results::RecoveryStatus) -> String {
    match s {
        crate::results::RecoveryStatus::Intact => "intact".into(),
        crate::results::RecoveryStatus::PartiallyRecovered => "partial".into(),
        crate::results::RecoveryStatus::Overwritten => "overwritten".into(),
        crate::results::RecoveryStatus::Carved => "carved".into(),
        crate::results::RecoveryStatus::CarvedValidated => "carved_validated".into(),
        crate::results::RecoveryStatus::PendingValidation => "pending".into(),
    }
}

#[cfg(test)]
mod stats_tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use crate::results::{RecoveredEntry, RecoveredEntryId, RecoveryStatus};

    fn entry(name: &str, ext: &str, size: u64) -> RecoveredEntry {
        RecoveredEntry {
            id: RecoveredEntryId(uuid::Uuid::new_v4()),
            fs_kind: "test".into(),
            parent_path: "/".into(),
            name: name.into(),
            size,
            start_lba: None,
            mtime: None,
            ctime: None,
            atime: None,
            status: RecoveryStatus::Intact,
            extension: ext.into(),
            mime: None,
            confidence: 1000,
            was_deleted: false,
            cluster_chain: vec![],
            extents: vec![],
            resident_data: None,
        }
    }

    #[test]
    fn categories_and_statuses_are_counted() {
        let store = crate::results::ResultsStore::new(100, None);
        store.insert(entry("a.jpg", "jpg", 10));
        store.insert(entry("b.mp4", "mp4", 100));
        store.insert(entry("c.zip", "zip", 1000));

        let stats = compute_stats(&store);
        let photos = stats
            .by_category
            .iter()
            .find(|(c, _, _)| *c == Category::photos)
            .unwrap();
        assert_eq!(photos.1, 1);
        let intact = stats
            .by_status
            .iter()
            .find(|(s, _, _)| s == "intact")
            .unwrap();
        assert_eq!(intact.1, 3);
    }

    #[test]
    fn stats_page_across_spilled_tiers() {
        // 10 entries with capacity 2 and a spill file: stats must see all.
        let dir = tempfile::tempdir().unwrap().keep();
        let store = crate::results::ResultsStore::new(2, Some(dir.join("s.bin")));
        for i in 0..10 {
            store.insert(entry(&format!("f{i}.jpg"), "jpg", i as u64 + 1));
        }
        let stats = compute_stats(&store);
        assert_eq!(stats.total_files, 10);
        assert_eq!(stats.total_bytes, 55);
    }
}
