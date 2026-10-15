//! Header/footer carving engine.
//!
//! Scans a byte window for file signatures and emits candidate matches.
//! This is the simplest carving strategy: scan for a known header magic,
//! then scan forward for the matching footer (if any). The candidate file
//! spans `[header_pos, footer_pos + footer_len)`.
//!
//! For production use, the candidate would then be validated by a
//! per-format analyser (e.g. JPEG analyser checks SOI/EOI markers, ZIP
//! analyser walks the central directory). For now, we just emit the
//! candidate with the signature's confidence weight.

use std::sync::Arc;

use tracing::{debug, trace};
use uuid::Uuid;

use dw_core::{
    ProgressEvent, ProgressPhase, ProgressSink, RecoveredEntry, RecoveredEntryId, RecoveryStatus,
    ResultsStore,
};
use dw_disk::ReadAt;
use dw_signatures::{Matcher, SignatureDb};

/// Carve files from a disk region by scanning for header/footer signatures.
///
/// Reads the disk in `chunk_size` chunks and scans each chunk for
/// signature matches. Matches are appended to `results` as
/// `RecoveredEntry`s with `status = Carved` and `was_deleted = true`.
///
/// # Arguments
///
/// * `read_at` - Synchronous reader for the disk.
/// * `start_offset` - Byte offset to start scanning from.
/// * `length` - Number of bytes to scan.
/// * `db` - The signature database.
/// * `results` - Where to append carved entries.
/// * `progress` - Progress sink.
/// * `scan_id` - Scan UUID for progress events.
/// * `chunk_size` - Read chunk size (default 1 MiB).
pub fn carve_region(
    read_at: &dyn ReadAt,
    start_offset: u64,
    length: u64,
    db: &SignatureDb,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
    scan_id: Uuid,
    chunk_size: usize,
) -> Result<u64, String> {
    let matcher = Matcher::new(db);
    let chunk_size = chunk_size.max(4096);
    let mut carved_count: u64 = 0;
    let mut offset = start_offset;
    let end = start_offset + length;
    let total_chunks = length.div_ceil(chunk_size as u64);

    let mut chunk_index: u64 = 0;

    while offset < end {
        let read_len = std::cmp::min(chunk_size as u64, end - offset) as usize;
        let mut buf = vec![0u8; read_len];

        match read_at.read_at(offset, &mut buf) {
            Ok(()) => {}
            Err(e) => {
                trace!(offset, error = %e, "Carver read error, skipping chunk");
                offset += read_len as u64;
                chunk_index += 1;
                continue;
            }
        }

        // Scan this chunk for header matches.
        let matches = matcher.find_headers(&buf, offset);

        for m in matches {
            // For now, create a carved entry for each match.
            // The entry's size is unknown without a footer scan or analyser —
            // we set it to 0 and let the UI show "size unknown" for carved files.
            // TODO: scan forward for a footer to estimate the file size.
            let entry = RecoveredEntry {
                id: RecoveredEntryId(Uuid::now_v7()),
                fs_kind: "carved".to_string(),
                parent_path: "[carved]".to_string(),
                name: format!("carved_{}.{}", m.offset, m.extension),
                size: 0, // unknown without footer scan
                start_lba: Some(m.offset / 512),
                mtime: None,
                ctime: None,
                atime: None,
                status: RecoveryStatus::Carved,
                extension: m.extension.clone(),
                mime: m.mime.clone(),
                confidence: m.weight,
                was_deleted: true,
                cluster_chain: Vec::new(),
                extents: Vec::new(),
                resident_data: None,
            };

            results.insert(entry);
            carved_count += 1;
        }

        chunk_index += 1;
        offset += read_len as u64;

        // Emit progress every 64 chunks.
        if chunk_index.is_multiple_of(64) {
            // Sync progress emission — the caller is in an async context.
            // We can't .await here, so we just log.
            debug!(chunk_index, total_chunks, carved_count, "Carver progress");
        }
    }

    // Emit final progress.
    // Note: this is a sync function, so we can't await the progress sink.
    // The caller handles async progress emission.
    let _ = progress;
    let _ = scan_id;

    Ok(carved_count)
}

/// Async wrapper for `carve_region` that emits progress events.
pub async fn carve_region_async(
    read_at: Arc<dyn ReadAt>,
    start_offset: u64,
    length: u64,
    db: Arc<SignatureDb>,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    scan_id: Uuid,
    chunk_size: usize,
) -> Result<u64, String> {
    let total_chunks = length.div_ceil(chunk_size.max(4096) as u64);

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::Carving,
            current: 0,
            total: total_chunks,
            message: format!("Starting carving pass ({length} bytes)"),
        })
        .await;

    let carved = carve_region(
        read_at.as_ref(),
        start_offset,
        length,
        &db,
        &results,
        &progress,
        scan_id,
        chunk_size,
    )?;

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::Carving,
            current: total_chunks,
            total: total_chunks,
            message: format!("Carving complete: {carved} files carved"),
        })
        .await;

    Ok(carved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_disk::{DiskSource, FileBackedSource};

    #[test]
    fn carve_finds_jpeg_header() {
        // Create a temp file with a JPEG header at offset 100.
        let mut data = vec![0u8; 4096];
        data[100] = 0xFF;
        data[101] = 0xD8;
        data[102] = 0xFF;
        data[103] = 0xE0;

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();

        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        struct NoSink;
        #[async_trait::async_trait]
        impl ProgressSink for NoSink {
            async fn emit(&self, _: ProgressEvent) {}
            async fn fail(&self, _: String) {}
            async fn done(&self) {}
        }

        let progress: Arc<dyn ProgressSink> = Arc::new(NoSink);
        let carved = carve_region(
            read_at.as_ref(),
            0,
            4096,
            &db,
            &results,
            &progress,
            Uuid::nil(),
            4096,
        )
        .unwrap();

        // Should find at least the JPEG match.
        assert!(
            carved >= 1,
            "expected at least 1 carved entry, got {carved}"
        );

        let snapshot = results.snapshot();
        let has_jpg = snapshot.iter().any(|e| e.extension == "jpg");
        assert!(has_jpg, "expected a .jpg carved entry");
    }

    #[test]
    fn carve_returns_zero_for_empty_region() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"").unwrap();

        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        struct NoSink;
        #[async_trait::async_trait]
        impl ProgressSink for NoSink {
            async fn emit(&self, _: ProgressEvent) {}
            async fn fail(&self, _: String) {}
            async fn done(&self) {}
        }

        let progress: Arc<dyn ProgressSink> = Arc::new(NoSink);
        let carved = carve_region(
            read_at.as_ref(),
            0,
            0,
            &db,
            &results,
            &progress,
            Uuid::nil(),
            4096,
        )
        .unwrap();

        assert_eq!(carved, 0);
    }
}
