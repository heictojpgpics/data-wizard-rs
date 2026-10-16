#![allow(clippy::panic)]
//! Engine validation over a synthetic volume mirroring the layouts
//! decoded from the real ReFS 3.14 sample: MSB+ pages carrying name
//! records and file-info records, plus a headless old-generation page
//! for the deep pass.

mod common;

use common::*;
use dw_core::results::ResultsStore;
use dw_core::{FsPlugin, ProbeOutcome, ScanMode};
use dw_refs::RefsPlugin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::runtime::Runtime;

fn no_progress() -> Arc<dyn dw_core::ProgressSink> {
    struct NoSink;
    #[async_trait::async_trait]
    impl dw_core::ProgressSink for NoSink {
        async fn emit(&self, _: dw_core::progress::ProgressEvent) {}
        async fn fail(&self, _: String) {}
        async fn done(&self) {}
    }
    Arc::new(NoSink)
}

fn volume(img: &[u8]) -> (Arc<dyn dw_disk::DiskSource>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("v.img");
    std::fs::write(&path, img).expect("write");
    let src = Arc::new(dw_disk::FileBackedSource::open(&path).expect("open"));
    (src, tmp)
}

fn scan_all(img: &[u8], mode: ScanMode) -> Vec<dw_core::results::RecoveredEntry> {
    let store = ResultsStore::new(1 << 20, None);
    let rt = Runtime::new().expect("rt");
    rt.block_on(async {
        let (src, _t) = volume(img);
        RefsPlugin
            .scan(
                src,
                mode,
                store.clone(),
                no_progress(),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .expect("refs scan");
    });
    let mut out = Vec::new();
    let mut off = 0usize;
    loop {
        let batch = store.page(off, 1000);
        if batch.is_empty() {
            break;
        }
        off += batch.len();
        out.extend(batch);
        if out.len() > 5000 {
            break;
        }
    }
    out
}

#[test]
fn probe_accepts_synthetic_volume() {
    let img = build_volume();
    let rt = Runtime::new().expect("rt");
    let (src, _t) = volume(&img);
    let outcome = rt
        .block_on(async { RefsPlugin.probe(src.as_ref()).await })
        .unwrap();
    match outcome {
        ProbeOutcome::Yes(p) => {
            assert_eq!(p.fs_kind, "refs");
            assert_eq!(p.cluster_size, 4096);
            assert_eq!(p.total_bytes, img.len() as u64);
        }
        other => panic!("expected Yes, got {other:?}"),
    }
}

#[test]
fn quick_pass_finds_live_files() {
    let img = build_volume();
    let entries = scan_all(&img, ScanMode::Quick);
    let by = |n: &str| entries.iter().find(|e| e.name == n);

    let hello = by("hello.txt").expect("hello.txt");
    assert!(!hello.was_deleted);
    assert_eq!(hello.size, 12);
    assert_eq!(hello.mtime.map(|t| t.unix_timestamp()), Some(1_700_000_000));
    assert_eq!(hello.extension, "txt");

    let inner = by("inner.bin").expect("inner.bin");
    assert_eq!(inner.size, 4096);
    assert_eq!(inner.mtime.map(|t| t.unix_timestamp()), Some(1_600_000_000));

    // both live; count matches the fixture
    assert_eq!(
        entries.len(),
        2,
        "{:?}",
        entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn deep_pass_surfaces_old_generation() {
    let img = build_volume();
    let entries = scan_all(&img, ScanMode::Deep);
    // The fixture writes an old-generation page with a file that was
    // deleted before the final state: gone.live must appear deleted.
    let gone = entries
        .iter()
        .find(|e| e.name == "gone.live")
        .expect("deep pass must surface gone.live");
    assert!(gone.was_deleted);
    assert_eq!(gone.size, 777);
    // and exactly one copy of it
    assert_eq!(entries.iter().filter(|e| e.name == "gone.live").count(), 1);
}

/// A structurally valid minimal JPEG (same construction the ext4
/// integration uses), used to prove deep-pass content carving.
fn minimal_jpeg() -> Vec<u8> {
    let mut jpg = vec![0xFF, 0xD8];
    jpg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
    jpg.extend_from_slice(&[0x00; 14]);
    jpg.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x45]);
    jpg.extend_from_slice(&[0x11; 67]);
    jpg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11]);
    jpg.extend_from_slice(&[0x22; 15]);
    jpg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C]);
    jpg.extend_from_slice(&[0x33; 10]);
    jpg.extend_from_slice(&vec![0x55u8; 3000]);
    jpg.extend_from_slice(&[0xFF, 0xD9]);
    jpg
}

#[test]
fn deep_pass_carves_content_from_headless_regions() {
    let mut img = build_volume();
    // Deleted content in a headless (non-headed) cluster.
    let jpg = minimal_jpeg();
    img[0x40000..0x40000 + jpg.len()].copy_from_slice(&jpg);

    let entries = scan_all(&img, ScanMode::Deep);
    let carved: Vec<_> = entries.iter().filter(|e| e.extension == "jpg").collect();
    assert!(
        !carved.is_empty(),
        "deep pass must carve the deleted JPEG content"
    );
    for j in &carved {
        assert_eq!(j.status, dw_core::results::RecoveryStatus::CarvedValidated);
        assert!(j.extents.iter().all(|x| x.start_byte >= 0x40000));
    }
}

#[test]
fn quick_pass_runs_no_carving() {
    let mut img = build_volume();
    let jpg = minimal_jpeg();
    img[0x40000..0x40000 + jpg.len()].copy_from_slice(&jpg);

    let entries = scan_all(&img, ScanMode::Quick);
    assert!(
        entries.iter().all(|e| e.extension != "jpg"),
        "quick pass must not carve content"
    );
}

#[test]
fn non_refs_volume_rejected() {
    let img = vec![0x41u8; 1 << 20];
    let rt = Runtime::new().expect("rt");
    let (src, _t) = volume(&img);
    let outcome = rt
        .block_on(async { RefsPlugin.probe(src.as_ref()).await })
        .unwrap();
    assert!(matches!(outcome, ProbeOutcome::No), "{outcome:?}");
}
