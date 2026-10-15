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
// Scan statistics (age map + category + status aggregations)
// ---------------------------------------------------------------------------

/// Aggregated statistics over a scan's results, computed with bounded
/// memory (the store is walked in pages, never fully snapshotted).
///
/// The month histogram drives the "bytes by last modified month" view:
/// entries without a usable mtime are counted separately so the UI can
/// say "N files have no recoverable timestamp" instead of hiding them.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanStats {
    pub total_files: u64,
    pub total_bytes: u64,
    /// Per status: count + bytes.
    pub by_status: Vec<(String, u64, u64)>,
    /// Per category: count + bytes.
    pub by_category: Vec<(Category, u64, u64)>,
    /// Per month ("YYYY-MM"): count + bytes, ascending.
    pub by_month: Vec<(String, u64, u64)>,
    /// Files whose mtime could not be recovered (carved or wiped metadata).
    pub no_timestamp_files: u64,
}

/// Compute stats over `results` by paging through it in batches.
#[must_use]
pub fn compute_stats(results: &crate::results::ResultsStore) -> ScanStats {
    use std::collections::BTreeMap;

    let mut stats = ScanStats::default();
    let mut months: BTreeMap<String, (u64, u64)> = BTreeMap::new();
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

            match e.mtime {
                Some(t) => {
                    let key = format!("{:04}-{:02}", t.year(), u8::from(t.month()));
                    let m = months.entry(key).or_insert((0, 0));
                    m.0 += 1;
                    m.1 += e.size;
                }
                None => stats.no_timestamp_files += 1,
            }
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
    stats.by_month = months.into_iter().map(|(k, (n, b))| (k, n, b)).collect();
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
    use time::OffsetDateTime;

    fn entry(name: &str, ext: &str, size: u64, mtime: Option<OffsetDateTime>) -> RecoveredEntry {
        RecoveredEntry {
            id: RecoveredEntryId(uuid::Uuid::new_v4()),
            fs_kind: "test".into(),
            parent_path: "/".into(),
            name: name.into(),
            size,
            start_lba: None,
            mtime,
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

    fn ts(year: i32, month: time::Month, day: u8) -> Option<OffsetDateTime> {
        Some(
            time::PrimitiveDateTime::new(
                time::Date::from_calendar_date(year, month, day).unwrap(),
                time::Time::from_hms(12, 0, 0).unwrap(),
            )
            .assume_utc(),
        )
    }

    #[test]
    fn months_aggregate_ascending() {
        let store = crate::results::ResultsStore::new(100, None);
        store.insert(entry("a.jpg", "jpg", 100, ts(2025, time::Month::March, 1)));
        store.insert(entry(
            "b.jpg",
            "jpg",
            200,
            ts(2024, time::Month::December, 1),
        ));
        store.insert(entry("c.pdf", "pdf", 50, ts(2025, time::Month::March, 2)));

        let stats = compute_stats(&store);
        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.total_bytes, 350);
        assert_eq!(stats.by_month.len(), 2);
        assert_eq!(stats.by_month[0].0, "2024-12");
        assert_eq!(stats.by_month[0].2, 200);
        assert_eq!(stats.by_month[1].0, "2025-03");
        assert_eq!(stats.by_month[1].1, 2);
        assert_eq!(stats.by_month[1].2, 150);
    }

    #[test]
    fn carved_files_without_mtime_are_counted_separately() {
        let store = crate::results::ResultsStore::new(100, None);
        store.insert(entry("x.jpg", "jpg", 10, None));
        store.insert(entry("y.jpg", "jpg", 20, ts(2025, time::Month::January, 1)));

        let stats = compute_stats(&store);
        assert_eq!(stats.no_timestamp_files, 1);
        assert_eq!(stats.by_month.len(), 1);
    }

    #[test]
    fn categories_and_statuses_are_counted() {
        let store = crate::results::ResultsStore::new(100, None);
        store.insert(entry("a.jpg", "jpg", 10, None));
        store.insert(entry("b.mp4", "mp4", 100, None));
        store.insert(entry("c.zip", "zip", 1000, None));

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
            store.insert(entry(&format!("f{i}.jpg"), "jpg", i as u64 + 1, None));
        }
        let stats = compute_stats(&store);
        assert_eq!(stats.total_files, 10);
        assert_eq!(stats.total_bytes, 55);
    }
}
