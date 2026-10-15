#![allow(clippy::panic)]
//! End-to-end engine test over a synthetic Btrfs volume built by hand:
//! 4 MiB, nodesize=sectorsize=4096, one SYSTEM|METADATA chunk (identity
//! mapped) + one DATA chunk (NON-identity mapped, so the test proves the
//! logical->physical translation), a live FS tree with regular /
//! fragmented / inline files, and an orphan old-generation leaf that the
//! deep pass must surface as deleted finds with names and extents.
//!
//! Every node and the superblock carry real crc32c checksums computed by
//! the engine's own crc32c(), so the csum verification path is genuinely
//! exercised (a wrong builder would fail the walk, not bypass it).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use dw_btrfs::superblock::crc32c;
use dw_core::progress::{ProgressEvent, ProgressSink};
use dw_core::results::{Extent, RecoveryStatus, ResultsStore};
use dw_core::{FsPlugin, ProbeOutcome, ScanMode};
use dw_disk::{DiskSource, FileBackedSource};

const NS: usize = 4096; // nodesize
const IMG: usize = 4 * 1024 * 1024;
/// DATA chunk: logical [1 MiB, 3 MiB) -> physical [0x181000, 0x381000)
const DATA_LOG: u64 = 1 << 20;
const DATA_PHYS: u64 = 0x1_81000;
/// 2025-01-01T00:00:00Z
const T: u64 = 1_735_689_600;

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

// ---- node builder --------------------------------------------------------

struct LeafItem {
    objectid: u64,
    key_type: u8,
    offset: u64,
    data: Vec<u8>,
}

/// Build one leaf node (level 0) with the given owner; computes csum.
/// Item data area starts at 0x400; item array at 101.
fn leaf(owner: u64, bytenr: u64, items: &[LeafItem]) -> Vec<u8> {
    let mut n = vec![0u8; NS];
    n[0x20..0x30].copy_from_slice(&[7u8; 16]); // fsid
    n[0x30..0x38].copy_from_slice(&bytenr.to_le_bytes());
    n[0x58..0x60].copy_from_slice(&owner.to_le_bytes());
    n[0x60..0x64].copy_from_slice(&(items.len() as u32).to_le_bytes());
    n[0x64] = 0; // level 0
    let mut data_at = 0x400usize;
    for (i, it) in items.iter().enumerate() {
        let base = 101 + 25 * i;
        n[base..base + 8].copy_from_slice(&it.objectid.to_le_bytes());
        n[base + 8] = it.key_type;
        n[base + 9..base + 17].copy_from_slice(&it.offset.to_le_bytes());
        n[base + 0x11..base + 0x15].copy_from_slice(&(data_at as u32).to_le_bytes());
        n[base + 0x15..base + 0x19].copy_from_slice(&(it.data.len() as u32).to_le_bytes());
        n[data_at..data_at + it.data.len()].copy_from_slice(&it.data);
        data_at += it.data.len();
    }
    let c = crc32c(&n[32..]);
    n[0..4].copy_from_slice(&c.to_le_bytes());
    n
}

// ---- item builders -------------------------------------------------------

fn inode_item(size: u64, mode: u32, mtime: Option<u64>) -> Vec<u8> {
    let mut d = vec![0u8; 160];
    d[0x10..0x18].copy_from_slice(&size.to_le_bytes());
    d[0x28..0x2C].copy_from_slice(&1u32.to_le_bytes()); // nlink
    d[0x34..0x38].copy_from_slice(&mode.to_le_bytes());
    if let Some(t) = mtime {
        d[0x88..0x90].copy_from_slice(&t.to_le_bytes());
    }
    d
}

fn dir_item(child: u64, name: &str, ft: u8) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&child.to_le_bytes());
    d.push(1); // child key type: INODE_ITEM
    d.extend_from_slice(&0u64.to_le_bytes());
    d.extend_from_slice(&1u64.to_le_bytes()); // transid
    d.extend_from_slice(&0u16.to_le_bytes()); // data_len
    d.extend_from_slice(&(name.len() as u16).to_le_bytes());
    d.push(ft);
    d.extend_from_slice(name.as_bytes());
    d
}

/// regular extent item: (disk_bytenr, data_offset, disk_num_bytes, num_bytes)
fn reg_extent(disk_bytenr: u64, data_offset: u64, disk_num: u64, num: u64) -> Vec<u8> {
    let mut d = vec![0u8; 53];
    d[0x10] = 0; // compression none
    d[0x14] = 1; // REG
    d[0x15..0x1D].copy_from_slice(&disk_bytenr.to_le_bytes());
    d[0x1D..0x25].copy_from_slice(&disk_num.to_le_bytes());
    d[0x25..0x2D].copy_from_slice(&data_offset.to_le_bytes());
    d[0x2D..0x35].copy_from_slice(&num.to_le_bytes());
    d
}

fn inline_extent(payload: &[u8]) -> Vec<u8> {
    let mut d = vec![0u8; 0x15];
    d[0x10] = 0;
    d[0x14] = 0; // INLINE
    d.extend_from_slice(payload);
    d
}

fn root_item(bytenr: u64, refs: u32) -> Vec<u8> {
    let mut d = vec![0u8; 0xEF];
    d[0xB0..0xB8].copy_from_slice(&bytenr.to_le_bytes());
    d[0xD8..0xDC].copy_from_slice(&refs.to_le_bytes());
    d[0xEE] = 0; // level
    d
}

/// sys-array entry: disk_key(17) + chunk(80)
fn sys_chunk(logical: u64, length: u64, physical: u64, profile: u64) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend_from_slice(&256u64.to_le_bytes());
    e.push(228);
    e.extend_from_slice(&logical.to_le_bytes());
    let mut c = [0u8; 80];
    c[0x00..0x08].copy_from_slice(&length.to_le_bytes());
    c[0x18..0x20].copy_from_slice(&profile.to_le_bytes());
    c[0x2C..0x2E].copy_from_slice(&1u16.to_le_bytes());
    c[0x38..0x40].copy_from_slice(&physical.to_le_bytes());
    e.extend_from_slice(&c);
    e
}

// ---- volume builder ------------------------------------------------------

fn build_image() -> Vec<u8> {
    let mut img = vec![0u8; IMG];

    // --- superblock at 0x10000 ---
    let mut sb = vec![0u8; 4096];
    sb[0x20..0x30].copy_from_slice(&[7u8; 16]); // fsid
    sb[0x30..0x38].copy_from_slice(&0x10000u64.to_le_bytes()); // bytenr
    sb[0x40..0x48].copy_from_slice(&0x4D5F_5366_5248_425Fu64.to_le_bytes()); // magic
    sb[0x48..0x50].copy_from_slice(&10u64.to_le_bytes()); // generation
    sb[0x50..0x58].copy_from_slice(&0x2000u64.to_le_bytes()); // root tree
    sb[0x58..0x60].copy_from_slice(&0x1000u64.to_le_bytes()); // chunk root
    sb[0x70..0x78].copy_from_slice(&(IMG as u64).to_le_bytes()); // total
    sb[0x88..0x90].copy_from_slice(&1u64.to_le_bytes()); // num devices
    sb[0x90..0x94].copy_from_slice(&4096u32.to_le_bytes());
    sb[0x94..0x98].copy_from_slice(&4096u32.to_le_bytes());
    // sys chunk array: SYSTEM|METADATA chunk only (bootstrap)
    let sys = sys_chunk(0, 1 << 20, 0, 0x6);
    let sys_len = sys.len() as u32;
    sb[0x32B..0x32B + sys.len()].copy_from_slice(&sys);
    sb[0xA0..0xA4].copy_from_slice(&sys_len.to_le_bytes());
    sb[0x12B..0x12B + 9].copy_from_slice(b"TESTBTRFS");
    sb[0xC4..0xC6].copy_from_slice(&0u16.to_le_bytes()); // crc32c
    let c = crc32c(&sb[32..4096]);
    sb[0..4].copy_from_slice(&c.to_le_bytes());
    img[0x10000..0x11000].copy_from_slice(&sb);

    // --- chunk tree at logical 0x1000 (identity chunk) ---
    let chunk_tree = leaf(
        3, // chunk tree owner
        0x1000,
        &[
            LeafItem {
                objectid: 256,
                key_type: 228,
                offset: 0,
                data: chunk_bytes(1 << 20, 0x6, 0),
            },
            LeafItem {
                objectid: 256,
                key_type: 228,
                offset: DATA_LOG,
                data: chunk_bytes(2 << 20, 0x1, DATA_PHYS),
            },
        ],
    );
    img[0x1000..0x2000].copy_from_slice(&chunk_tree);

    // --- root tree at 0x2000: FS tree (5) at 0x3000 ---
    let root_tree = leaf(
        1, // root tree owner
        0x2000,
        &[LeafItem {
            objectid: 5,
            key_type: 132,
            offset: 0,
            data: root_item(0x3000, 1),
        }],
    );
    img[0x2000..0x3000].copy_from_slice(&root_tree);

    // --- FS tree at 0x3000, owner 5 ---
    let fs_items = vec![
        // root dir inode + entries
        LeafItem {
            objectid: 256,
            key_type: 1,
            offset: 0,
            data: inode_item(0, 0x41ED, None),
        },
        LeafItem {
            objectid: 256,
            key_type: 84,
            offset: 1,
            data: dir_item(300, "sub", 2),
        },
        LeafItem {
            objectid: 256,
            key_type: 84,
            offset: 2,
            data: dir_item(257, "hello.txt", 1),
        },
        LeafItem {
            objectid: 256,
            key_type: 84,
            offset: 3,
            data: dir_item(259, "tiny.txt", 1),
        },
        // hello.txt: regular, 12 bytes at DATA logical 0x101000
        LeafItem {
            objectid: 257,
            key_type: 1,
            offset: 0,
            data: inode_item(12, 0x81A4, Some(T)),
        },
        LeafItem {
            objectid: 257,
            key_type: 108,
            offset: 0,
            data: reg_extent(DATA_LOG + 0x1000, 0, 4096, 4096),
        },
        // sub dir + inner.bin (fragmented: two extents, non-contiguous)
        LeafItem {
            objectid: 300,
            key_type: 1,
            offset: 0,
            data: inode_item(0, 0x41ED, None),
        },
        LeafItem {
            objectid: 300,
            key_type: 84,
            offset: 1,
            data: dir_item(301, "inner.bin", 1),
        },
        LeafItem {
            objectid: 301,
            key_type: 1,
            offset: 0,
            data: inode_item(8192, 0x81A4, Some(T)),
        },
        LeafItem {
            objectid: 301,
            key_type: 108,
            offset: 0,
            data: reg_extent(DATA_LOG + 0x3000, 0, 4096, 4096),
        },
        LeafItem {
            objectid: 301,
            key_type: 108,
            offset: 4096,
            data: reg_extent(DATA_LOG + 0x5000, 0, 4096, 4096),
        },
        // tiny.txt: inline, 8 bytes
        LeafItem {
            objectid: 259,
            key_type: 1,
            offset: 0,
            data: inode_item(8, 0x81A4, Some(T)),
        },
        LeafItem {
            objectid: 259,
            key_type: 108,
            offset: 0,
            data: inline_extent(b"tinypay!"),
        },
    ];
    let fs_tree = leaf(5, 0x3000, &fs_items);
    img[0x3000..0x4000].copy_from_slice(&fs_tree);

    // --- orphan old-generation leaf at 0x6000, owner 5 ---
    let orphan_items = vec![
        LeafItem {
            objectid: 256,
            key_type: 84,
            offset: 9,
            data: dir_item(512, "ghost.bin", 1),
        },
        LeafItem {
            objectid: 512,
            key_type: 1,
            offset: 0,
            data: inode_item(20, 0x81A4, Some(T + 86400)),
        },
        LeafItem {
            objectid: 512,
            key_type: 108,
            offset: 0,
            data: reg_extent(DATA_LOG + 0x7000, 0, 4096, 4096),
        },
    ];
    let orphan = leaf(5, 0x6000, &orphan_items);
    img[0x6000..0x7000].copy_from_slice(&orphan);

    // --- file payloads at their PHYSICAL locations ---
    let hello_phys = DATA_PHYS + 0x1000; // 0x182000
    img[hello_phys as usize..hello_phys as usize + 12].copy_from_slice(b"hello world\n");
    let p1 = DATA_PHYS + 0x3000;
    img[p1 as usize..p1 as usize + 4096].fill(0xAA);
    let p2 = DATA_PHYS + 0x5000;
    img[p2 as usize..p2 as usize + 4096].fill(0xBB);
    let ghost_phys = DATA_PHYS + 0x7000;
    img[ghost_phys as usize..ghost_phys as usize + 20].copy_from_slice(b"ghost data here!!!\0\0");

    img
}

/// chunk item bytes for the chunk TREE (key offset carries logical).
fn chunk_bytes(length: u64, profile: u64, physical: u64) -> Vec<u8> {
    let mut c = vec![0u8; 80];
    c[0x00..0x08].copy_from_slice(&length.to_le_bytes());
    c[0x18..0x20].copy_from_slice(&profile.to_le_bytes());
    c[0x2C..0x2E].copy_from_slice(&1u16.to_le_bytes());
    c[0x38..0x40].copy_from_slice(&physical.to_le_bytes());
    c
}

// ---- harness --------------------------------------------------------------

async fn scan_image(mode: ScanMode) -> Arc<ResultsStore> {
    let img = build_image();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &img).unwrap();
    let source: Arc<dyn DiskSource> = Arc::new(FileBackedSource::open(tmp.path()).unwrap());
    let results = ResultsStore::new(10_000, None);
    let cancel = Arc::new(AtomicBool::new(false));
    dw_btrfs::plugin::BtrfsPlugin
        .scan(source, mode, results.clone(), no_progress(), cancel)
        .await
        .expect("btrfs scan");
    results
}

fn find(store: &ResultsStore, name: &str) -> Vec<dw_core::results::RecoveredEntry> {
    store
        .page(0, 10_000)
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn probe_classifies_btrfs() {
    let img = build_image();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &img).unwrap();
    let source: Arc<dyn DiskSource> = Arc::new(FileBackedSource::open(tmp.path()).unwrap());
    let out = dw_btrfs::plugin::BtrfsPlugin
        .probe(source.as_ref())
        .await
        .unwrap();
    match out {
        ProbeOutcome::Yes(p) => {
            assert_eq!(p.fs_kind, "btrfs");
            assert_eq!(p.volume_label.as_deref(), Some("TESTBTRFS"));
            assert_eq!(p.cluster_size, 4096);
            assert_eq!(p.total_bytes, 4 * 1024 * 1024);
        }
        other => panic!("expected Yes, got {other:?}"),
    }
}

#[tokio::test]
async fn quick_scan_maps_data_chunk_and_orders_fragments() {
    let store = scan_image(ScanMode::Quick).await;
    // hello.txt: 12 bytes at DATA logical 0x101000 -> phys 0x182000
    let e = &find(&store, "hello.txt")[0];
    assert_eq!(e.size, 12);
    assert_eq!(
        e.extents,
        vec![Extent {
            start_byte: DATA_PHYS + 0x1000,
            length: 12
        }]
    );
    assert!(!e.was_deleted);
    assert_eq!(e.status, RecoveryStatus::Intact);
    assert_eq!(e.parent_path, "/");
    assert_eq!(e.mtime.map(|t| t.unix_timestamp()), Some(T as i64));

    // inner.bin: fragmented in file-logical order through the DATA chunk
    let e = &find(&store, "inner.bin")[0];
    assert_eq!(e.size, 8192);
    assert_eq!(
        e.extents,
        vec![
            Extent {
                start_byte: DATA_PHYS + 0x3000,
                length: 4096
            },
            Extent {
                start_byte: DATA_PHYS + 0x5000,
                length: 4096
            },
        ]
    );
    assert_eq!(e.parent_path, "/sub");
}

#[tokio::test]
async fn quick_scan_inline_file_is_resident() {
    let store = scan_image(ScanMode::Quick).await;
    let e = &find(&store, "tiny.txt")[0];
    assert_eq!(e.size, 8);
    assert_eq!(e.resident_data.as_deref(), Some(b"tinypay!".as_slice()));
    assert!(e.extents.is_empty());
    assert_eq!(e.status, RecoveryStatus::Intact);
}

#[tokio::test]
async fn quick_mode_does_not_surface_orphans() {
    let store = scan_image(ScanMode::Quick).await;
    assert!(
        find(&store, "ghost.bin").is_empty(),
        "orphan pass must not run in quick"
    );
    assert_eq!(store.page(0, 10_000).len(), 3); // hello, inner, tiny
}

#[tokio::test]
async fn deep_scan_surfaces_orphan_with_name_and_extent() {
    let store = scan_image(ScanMode::Deep).await;
    let g = find(&store, "ghost.bin");
    assert_eq!(g.len(), 1, "orphan leaf must surface exactly once");
    let e = &g[0];
    assert!(e.was_deleted);
    assert_eq!(e.size, 20);
    assert_eq!(
        e.extents,
        vec![Extent {
            start_byte: DATA_PHYS + 0x7000,
            length: 20
        }]
    );
    assert_eq!(e.parent_path, "/");
}

#[tokio::test]
async fn deep_scan_does_not_duplicate_live_entries() {
    let store = scan_image(ScanMode::Deep).await;
    let all = store.page(0, 10_000);
    for name in ["hello.txt", "inner.bin", "tiny.txt", "ghost.bin"] {
        let n = all.iter().filter(|e| e.name == name).count();
        assert_eq!(n, 1, "{name} emitted {n} times");
    }
}
