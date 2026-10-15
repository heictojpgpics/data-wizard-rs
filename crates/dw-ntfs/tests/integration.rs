// Integration tests legitimately use assert! / panic! — that's how tests work.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

//! Integration test: scan a real NTFS image and verify recovery.
//!
//! This test is `#[ignore]` by default because it requires a real NTFS
//! image file. The CI verification workflow creates a VHD, passes its
//! path via the `DW_TEST_IMAGE` env var, and runs:
//!
//! ```sh
//! cargo test --package dw-ntfs --test integration -- --ignored --nocapture
//! ```

use std::sync::Arc;

use dw_core::{FsPlugin, ProbeOutcome, ProgressEvent, ProgressSink, ResultsStore, ScanMode};
use dw_disk::FileBackedSource;

struct NoOpSink;

#[async_trait::async_trait]
impl ProgressSink for NoOpSink {
    async fn emit(&self, _event: ProgressEvent) {}
    async fn fail(&self, _message: String) {}
    async fn done(&self) {}
}

/// Scan a real NTFS image and verify the scanner finds entries.
///
/// Prerequisites:
/// - The `DW_TEST_IMAGE` env var must point to an NTFS image file.
/// - The image should contain at least 10 files (the CI workflow
///   creates 100 files and deletes 50).
///
/// This test verifies:
/// 1. The probe succeeds (ProbeOutcome::Yes).
/// 2. The scan completes without error.
/// 3. The results store contains at least 10 entries.
/// 4. At least some entries have names.
#[tokio::test]
#[ignore]
async fn scan_ntfs_image() {
    let image_path = std::env::var("DW_TEST_IMAGE")
        .expect("DW_TEST_IMAGE env var must be set to an NTFS image path");

    let source = Arc::new(
        FileBackedSource::open(std::path::Path::new(&image_path)).expect("failed to open image"),
    );

    let plugin = dw_ntfs::NtfsPlugin;

    // Probe
    let probe = plugin
        .probe(source.as_ref())
        .await
        .expect("probe should not error");
    match probe {
        ProbeOutcome::Yes(info) => {
            assert_eq!(info.fs_kind, "ntfs");
            println!(
                "Probe OK: serial={:?}, size={}, cluster={}",
                info.volume_serial, info.total_bytes, info.cluster_size
            );
        }
        ProbeOutcome::No => panic!("Probe rejected the image — not NTFS?"),
        ProbeOutcome::Corrupt(msg) => panic!("Probe found corrupt NTFS: {msg}"),
    }

    // Scan
    let results = ResultsStore::new(1_000_000, None);
    let progress = Arc::new(NoOpSink);
    plugin
        .scan(source, ScanMode::Quick, results.clone(), progress)
        .await
        .expect("scan should not error");

    // Verify results
    let snapshot = results.snapshot();
    println!("Recovered {} entries", snapshot.len());

    assert!(
        snapshot.len() >= 10,
        "expected at least 10 entries, got {}",
        snapshot.len()
    );

    // At least some entries should have non-empty names
    let with_names = snapshot.iter().filter(|e| !e.name.is_empty()).count();
    assert!(
        with_names >= 5,
        "expected at least 5 entries with names, got {with_names}"
    );

    // Print a summary
    let deleted = snapshot.iter().filter(|e| e.was_deleted).count();
    let live = snapshot.iter().filter(|e| !e.was_deleted).count();
    println!("Live: {live}, Deleted: {deleted}");

    // Print first 20 for debugging
    for entry in snapshot.iter().take(20) {
        let status = if entry.was_deleted { "DEL" } else { "   " };
        println!(
            "  [{status}] {}/{} ({} bytes)",
            entry.parent_path, entry.name, entry.size
        );
    }
}

/// Verify that the NTFS MFT walker handles a missing/empty image
/// gracefully (should return an error, not panic).
#[tokio::test]
async fn scan_nonexistent_image_errors() {
    let path = std::path::Path::new("/nonexistent/path/to/image.img");
    let result = FileBackedSource::open(path);
    assert!(result.is_err());
}
