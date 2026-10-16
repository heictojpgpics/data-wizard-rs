#![allow(clippy::panic)]
//! End-to-end engine validation over the synthetic pool.

mod common;

use common::*;
use dw_core::progress::ProgressEvent;
use dw_core::results::ResultsStore;
use dw_core::{FsPlugin, ProbeOutcome, ProgressSink, ScanMode};
use dw_disk::DiskSource;
use dw_zfs::ZfsPlugin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::runtime::Runtime;

fn run<F>(f: F)
where
    F: FnOnce(&Runtime),
{
    let rt = Runtime::new().expect("tokio runtime");
    f(&rt);
}

fn no_progress() -> Arc<dyn ProgressSink> {
    struct NoSink;
    #[async_trait::async_trait]
    impl ProgressSink for NoSink {
        async fn emit(&self, _: ProgressEvent) {}
        async fn fail(&self, _: String) {}
        async fn done(&self) {}
    }
    Arc::new(NoSink)
}

/// The engines read through `DiskSource`; write the image to a temp file
/// and open it (same harness the btrfs suite uses).
fn file_source(img: &[u8]) -> (Arc<dyn DiskSource>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("pool.img");
    std::fs::write(&path, img).expect("write image");
    let src = Arc::new(dw_disk::FileBackedSource::open(&path).expect("open source"));
    (src, tmp)
}

/// Collect entry names -> (parent_path, size, deleted, mtime).
fn scan_all(img: &[u8], mode: ScanMode) -> Vec<dw_core::results::RecoveredEntry> {
    let store = ResultsStore::new(1 << 20, None);
    run(|rt| {
        rt.block_on(async {
            let (src, _tmp) = file_source(img);
            ZfsPlugin
                .scan(
                    src,
                    mode,
                    store.clone(),
                    no_progress(),
                    Arc::new(AtomicBool::new(false)),
                )
                .await
                .expect("zfs scan");
        });
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
fn probe_recognizes_pool() {
    let img = build_pool();
    run(|rt| {
        let (src, _tmp) = file_source(&img);
        let outcome = rt
            .block_on(async { ZfsPlugin.probe(src.as_ref()).await })
            .expect("probe");
        match outcome {
            ProbeOutcome::Yes(p) => {
                assert_eq!(p.fs_kind, "zfs");
                assert_eq!(p.volume_label.as_deref(), Some("testpool"));
                assert_eq!(p.total_bytes, POOL_SIZE);
            }
            other => panic!("expected Yes, got {other:?}"),
        }
    });
}

#[test]
fn quick_walk_finds_live_entries() {
    let img = build_pool();
    let entries = scan_all(&img, ScanMode::Quick);
    let by_name: Vec<(String, String, u64, bool)> = entries
        .iter()
        .map(|e| (e.name.clone(), e.parent_path.clone(), e.size, e.was_deleted))
        .collect();

    // hello.txt: live, size 12, LZ4 block extent, mtime from SA
    let hello = entries
        .iter()
        .find(|e| e.name == "hello.txt")
        .expect("hello.txt");
    assert_eq!(hello.parent_path, "/");
    assert_eq!(hello.size, HELLO_SIZE);
    assert!(!hello.was_deleted);
    assert_eq!(hello.extents.len(), 1);
    assert_eq!(hello.extents[0].start_byte, OFF_HELLO_DATA);
    assert_eq!(hello.extents[0].length, 512);
    assert_eq!(
        hello.mtime.map(|t| t.unix_timestamp()),
        Some(HELLO_MTIME as i64)
    );

    // sub dir: live
    let sub = entries.iter().find(|e| e.name == "sub").expect("sub");
    assert_eq!(sub.parent_path, "/");
    assert!(!sub.was_deleted);

    // inner.bin is NOT in the newest tree
    assert!(entries.iter().all(|e| e.name != "inner.bin"), "{by_name:?}");
    assert_eq!(entries.len(), 2, "{by_name:?}");
}

#[test]
fn deep_pass_recovers_deleted_file() {
    let img = build_pool();
    let entries = scan_all(&img, ScanMode::Deep);
    let inner = entries
        .iter()
        .find(|e| e.name == "inner.bin")
        .expect("deep pass must surface inner.bin");
    assert!(inner.was_deleted, "inner.bin must be marked deleted");
    assert_eq!(inner.parent_path, "/sub");
    assert_eq!(inner.size, 4096);
    assert_eq!(inner.extents.len(), 1);
    assert_eq!(inner.extents[0].start_byte, OFF_INNER_DATA);
    assert_eq!(inner.extents[0].length, 4096);
    assert_eq!(
        inner.mtime.map(|t| t.unix_timestamp()),
        Some(INNER_MTIME as i64)
    );

    // hello.txt and sub remain live (never duplicated as deleted)
    let hello_count = entries.iter().filter(|e| e.name == "hello.txt").count();
    assert_eq!(hello_count, 1);
    assert!(
        entries
            .iter()
            .any(|e| e.name == "hello.txt" && !e.was_deleted),
        "hello.txt must stay live"
    );
}

#[test]
fn fatzap_root_dir_entries_surface() {
    // The root dir is a fatzap (16 KiB header, embedded ptr table, leaf);
    // hello.txt and sub only appear if the fatzap path works.
    let img = build_pool();
    let entries = scan_all(&img, ScanMode::Quick);
    assert!(entries.iter().any(|e| e.name == "hello.txt"));
    assert!(entries.iter().any(|e| e.name == "sub"));
}

#[test]
fn corrupted_label_checksum_falls_back() {
    let mut img = build_pool();
    // Flip a byte inside the L0 nvlist body (before the eck).
    img[16 * 1024 + 200] ^= 0xFF;
    run(|rt| {
        let outcome = rt
            .block_on(async {
                let (src, _t) = file_source(&img);
                ZfsPlugin.probe(src.as_ref()).await
            })
            .expect("probe");
        // One label corrupted: the other three still verify, so the pool
        // is still recognized (real ZFS behaves the same way).
        assert!(matches!(outcome, ProbeOutcome::Yes(_)), "{outcome:?}");
    });
}

#[test]
fn all_labels_corrupt_is_rejected() {
    let mut img = build_pool();
    for base in [
        0u64,
        256 * 1024,
        POOL_SIZE - 512 * 1024,
        POOL_SIZE - 256 * 1024,
    ] {
        let at = base as usize + 16 * 1024 + 300;
        img[at] ^= 0xFF;
    }
    run(|rt| {
        let outcome = rt
            .block_on(async {
                let (src, _t) = file_source(&img);
                ZfsPlugin.probe(src.as_ref()).await
            })
            .expect("probe");
        assert!(
            matches!(outcome, ProbeOutcome::No | ProbeOutcome::Corrupt(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn non_zfs_image_is_rejected() {
    let img = vec![0x41u8; 1 << 20];
    run(|rt| {
        let outcome = rt
            .block_on(async {
                let (src, _t) = file_source(&img);
                ZfsPlugin.probe(src.as_ref()).await
            })
            .expect("probe");
        assert!(matches!(outcome, ProbeOutcome::No), "{outcome:?}");
    });
}

#[test]
fn multi_vdev_pool_is_out_of_scope() {
    let mut img = build_pool();
    // Rewrite the vdev_tree in all labels to a two-disk children array.
    let child = {
        let mut c = Vec::new();
        nv_pub_header(&mut c);
        nv_pub_pair_str(&mut c, "type", "disk");
        nv_pub_pair_u64(&mut c, "guid", 2);
        nv_pub_end(&mut c);
        c
    };
    let tree = {
        let mut t = Vec::new();
        nv_pub_header(&mut t);
        nv_pub_pair_str(&mut t, "type", "root");
        nv_pub_pair_u64(&mut t, "ashift", 9);
        nv_pub_pair_nvlist_array_pub(&mut t, "children", &[child.clone(), child]);
        nv_pub_end(&mut t);
        t
    };
    let mut n = Vec::new();
    nv_pub_header(&mut n);
    nv_pub_pair_str(&mut n, "name", "testpool");
    nv_pub_pair_u64(&mut n, "version", 5000);
    nv_pub_pair_u64(&mut n, "pool_guid", 1);
    nv_pub_pair_nvlist_pub(&mut n, "vdev_tree", &tree);
    nv_pub_end(&mut n);

    let region = seal_vdev_phys(&n);
    for base in [
        0u64,
        256 * 1024,
        POOL_SIZE - 512 * 1024,
        POOL_SIZE - 256 * 1024,
    ] {
        let at = base as usize;
        img[at + 16 * 1024..at + 16 * 1024 + region.len()].copy_from_slice(&region);
    }
    run(|rt| {
        let outcome = rt
            .block_on(async {
                let (src, _t) = file_source(&img);
                ZfsPlugin.probe(src.as_ref()).await
            })
            .expect("probe");
        match outcome {
            ProbeOutcome::Corrupt(reason) => {
                assert!(reason.contains("raw carving"), "{reason}");
            }
            other => panic!("expected Corrupt(out-of-scope), got {other:?}"),
        }
    });
}

// Public re-exports of the builder internals for the multi-vdev test.
fn nv_pub_header(n: &mut Vec<u8>) {
    n.extend_from_slice(&1u32.to_le_bytes());
    n.extend_from_slice(&0u32.to_le_bytes());
    n.extend_from_slice(&0u32.to_le_bytes());
}
fn nv_pub_end(n: &mut Vec<u8>) {
    n.extend_from_slice(&0u32.to_le_bytes());
    n.extend_from_slice(&0u32.to_le_bytes());
}
fn nv_pub_pair_str(n: &mut Vec<u8>, name: &str, value: &str) {
    let mut body = Vec::new();
    body.extend_from_slice(&(name.len() as u32).to_le_bytes());
    let before = body.len();
    body.extend_from_slice(name.as_bytes());
    while (body.len() - before) % 4 != 0 {
        body.push(0);
    }
    body.extend_from_slice(&9u32.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&(value.len() as u32).to_le_bytes());
    let before = body.len();
    body.extend_from_slice(value.as_bytes());
    while (body.len() - before) % 4 != 0 {
        body.push(0);
    }
    n.extend_from_slice(&(8 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&(24 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&body);
}
fn nv_pub_pair_u64(n: &mut Vec<u8>, name: &str, value: u64) {
    let mut body = Vec::new();
    body.extend_from_slice(&(name.len() as u32).to_le_bytes());
    let before = body.len();
    body.extend_from_slice(name.as_bytes());
    while (body.len() - before) % 4 != 0 {
        body.push(0);
    }
    body.extend_from_slice(&8u32.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&value.to_le_bytes());
    n.extend_from_slice(&(8 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&(24 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&body);
}
fn nv_pub_pair_nvlist_pub(n: &mut Vec<u8>, name: &str, nested: &[u8]) {
    let mut body = Vec::new();
    body.extend_from_slice(&(name.len() as u32).to_le_bytes());
    let before = body.len();
    body.extend_from_slice(name.as_bytes());
    while (body.len() - before) % 4 != 0 {
        body.push(0);
    }
    body.extend_from_slice(&19u32.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(nested);
    n.extend_from_slice(&(8 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&(24 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&body);
}
fn nv_pub_pair_nvlist_array_pub(n: &mut Vec<u8>, name: &str, items: &[Vec<u8>]) {
    let mut body = Vec::new();
    body.extend_from_slice(&(name.len() as u32).to_le_bytes());
    let before = body.len();
    body.extend_from_slice(name.as_bytes());
    while (body.len() - before) % 4 != 0 {
        body.push(0);
    }
    body.extend_from_slice(&20u32.to_le_bytes());
    body.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for it in items {
        body.extend_from_slice(it);
    }
    n.extend_from_slice(&(8 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&(24 + body.len() as u32).to_le_bytes());
    n.extend_from_slice(&body);
}

/// Build the full 112 KiB vdev_phys region from a nvlist body.
fn seal_vdev_phys(body: &[u8]) -> Vec<u8> {
    let mut region = vec![0u8; 112 * 1024];
    region[..body.len()].copy_from_slice(body);
    let c = dw_zfs::checksum::label_fletcher4(&region);
    let eck_at = region.len() - 40;
    region[eck_at + 8..eck_at + 40].copy_from_slice(&c.0.map(u64::to_le_bytes).concat()[..32]);
    region
}
