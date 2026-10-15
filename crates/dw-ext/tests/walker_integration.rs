//! End-to-end engine test over a synthetic ext4 volume built by hand
//! (1 MiB, 256 x 4 KiB blocks, 1 group, ext4 with EXTENTS+FILETYPE).
//!
#![allow(clippy::panic)]

//! Covered behaviors:
//!
//! * probe: fs_kind ext4, volume label, geometry
//! * quick: live walk (fragmented file in logical order, per-inode
//!   timestamps incl. crtime) plus the shadow-dirent pass (deleted.txt
//!   named by surviving dirent bytes)
//! * deep: orphan pass (unnamed unallocated inode surfaced by number)
//!   plus free-run carving (a structurally valid JPEG placed only in
//!   free blocks)
//! * negative: quick mode does NOT run the orphan or carve passes

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use dw_core::progress::{ProgressEvent, ProgressSink};
use dw_core::results::{Extent, RecoveryStatus, ResultsStore};
use dw_core::{FsPlugin, ProbeOutcome, ScanMode};
use dw_disk::{DiskSource, FileBackedSource};
use dw_ext::plugin::ExtPlugin;

const BS: usize = 4096;
const BLOCKS: usize = 256;
/// 2025-01-01T00:00:00Z
const T: u32 = 1_735_689_600;
/// 2025-01-02T00:00:00Z (dtime)
const T2: u32 = 1_735_776_000;

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

// ---- image builder ------------------------------------------------------

fn u16p(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn u32p(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// dirent: inode u32 | rec_len u16 | name_len u8 | type u8 | name+pad
fn dirent(inode: u32, name: &str, rec_len: u16, ftype: u8) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend_from_slice(&inode.to_le_bytes());
    e.extend_from_slice(&rec_len.to_le_bytes());
    e.push(name.len() as u8);
    e.push(ftype);
    let mut pad = name.as_bytes().to_vec();
    assert!(
        pad.len() + 8 <= usize::from(rec_len),
        "rec_len too small for {name:?}"
    );
    pad.resize(usize::from(rec_len) - 8, 0);
    e.extend_from_slice(&pad);
    e
}

/// extent-tree root with the given leaf entries: (logical, phys, len).
fn extent_root(entries: &[(u32, u64, u16)]) -> [u8; 60] {
    let mut ib = [0u8; 60];
    ib[0..2].copy_from_slice(&0xF30Au16.to_le_bytes());
    ib[2..4].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    ib[4..6].copy_from_slice(&4u16.to_le_bytes()); // eh_max
    ib[6..8].copy_from_slice(&0u16.to_le_bytes()); // depth 0
    for (i, &(lg, phys, len)) in entries.iter().enumerate() {
        let o = 12 + i * 12;
        u32p(&mut ib, o, lg);
        u16p(&mut ib, o + 4, len);
        u16p(&mut ib, o + 6, (phys >> 32) as u16);
        u32p(&mut ib, o + 8, phys as u32);
    }
    ib
}

/// One inode record (256 bytes) with the fields the engine consumes.
fn inode_rec(
    mode: u16,
    size: u64,
    links: u16,
    flags: u32,
    i_block: &[u8; 60],
    atime: u32,
    ctime: u32,
    mtime: u32,
    dtime: u32,
    crtime: u32,
) -> Vec<u8> {
    let mut b = vec![0u8; 256];
    u16p(&mut b, 0x00, mode);
    u32p(&mut b, 0x04, size as u32);
    u32p(&mut b, 0x6C, (size >> 32) as u32);
    u16p(&mut b, 0x1A, links);
    u32p(&mut b, 0x20, flags);
    b[0x28..0x64].copy_from_slice(i_block);
    u32p(&mut b, 0x08, atime);
    u32p(&mut b, 0x0C, ctime);
    u32p(&mut b, 0x10, mtime);
    u32p(&mut b, 0x14, dtime);
    u32p(&mut b, 0x1C, 8); // i_blocks (4 KiB in 512B sectors)
    // extra fields: extra_isize 0x30, _extra = 0 (no nsec, epoch hi 0)
    u16p(&mut b, 0x80, 0x30);
    u32p(&mut b, 0x90, crtime);
    b
}

fn set_bit(buf: &mut [u8], n: usize) {
    buf[n / 8] |= 1 << (n % 8);
}

fn build_image() -> Vec<u8> {
    let mut img = vec![0u8; BLOCKS * BS];

    // --- superblock at 1024 inside block 0 ---
    let mut sb = [0u8; 1024];
    u32p(&mut sb, 0x00, 16); // inodes_count
    u32p(&mut sb, 0x04, 256); // blocks_count
    u32p(&mut sb, 0x0C, 246); // free blocks (10 used)
    u32p(&mut sb, 0x10, 11); // free inodes (16 - 5 used)
    u32p(&mut sb, 0x14, 0); // first_data_block
    u32p(&mut sb, 0x18, 2); // log block size -> 4096
    u32p(&mut sb, 0x20, 256); // blocks_per_group
    u32p(&mut sb, 0x28, 16); // inodes_per_group
    u16p(&mut sb, 0x34, 1); // mnt_count
    u16p(&mut sb, 0x38, 0xEF53); // magic
    u16p(&mut sb, 0x3A, 1); // state clean
    u32p(&mut sb, 0x4C, 1); // rev 1
    u16p(&mut sb, 0x58, 256); // inode size
    u32p(&mut sb, 0x5C, 0); // compat
    u32p(&mut sb, 0x60, 0x42); // incompat: FILETYPE|EXTENTS -> ext4
    u32p(&mut sb, 0x64, 0); // ro_compat
    sb[0x68..0x78].copy_from_slice(&[7u8; 16]); // uuid
    sb[0x78..0x80].copy_from_slice(b"TESTEXT4"); // volume name (16B field)
    img[1024..2048].copy_from_slice(&sb);

    // --- group descriptor at block 1 ---
    let mut gd = vec![0u8; 32];
    u32p(&mut gd, 0x00, 2); // block bitmap
    u32p(&mut gd, 0x04, 3); // inode bitmap
    u32p(&mut gd, 0x08, 4); // inode table
    u16p(&mut gd, 0x0C, 246); // free blocks
    u16p(&mut gd, 0x0E, 11); // free inodes
    u16p(&mut gd, 0x10, 2); // used dirs
    img[BS..BS + 32].copy_from_slice(&gd);

    // --- block bitmap at block 2 ---
    let mut bb = vec![0u8; BS];
    for b in [0usize, 1, 2, 3, 4, 10, 11, 12, 22, 24] {
        set_bit(&mut bb, b);
    }
    // padding bits beyond the volume (256..32767) are marked used, as
    // mkfs does, so they are never carved.
    for n in BLOCKS..BS * 8 {
        set_bit(&mut bb, n);
    }
    img[2 * BS..3 * BS].copy_from_slice(&bb);

    // --- inode bitmap at block 3 ---
    let mut ibm = vec![0u8; BS];
    for i in [0usize, 1, 2, 4, 5] {
        // inodes 1, 2, 3, 5, 6
        set_bit(&mut ibm, i);
    }
    for n in 16..BS * 8 {
        set_bit(&mut ibm, n);
    }
    img[3 * BS..4 * BS].copy_from_slice(&ibm);

    // --- inode table at block 4 (8 x 256 = 2048 bytes) ---
    let mut table = Vec::new();
    table.extend_from_slice(&[0u8; 256]); // inode 1: reserved
    table.extend_from_slice(&inode_rec(
        0x41ED,
        4096,
        3,
        0x80000,
        &extent_root(&[(0, 10, 1)]),
        T,
        T,
        T,
        0,
        T,
    )); // 2: root
    table.extend_from_slice(&inode_rec(
        0x81A4,
        12,
        1,
        0x80000,
        &extent_root(&[(0, 11, 1)]),
        T,
        T,
        T,
        0,
        T,
    )); // 3: hello.txt
    table.extend_from_slice(&inode_rec(
        0x81A4,
        20,
        0,
        0x80000,
        &extent_root(&[(0, 20, 1)]),
        T,
        T,
        T,
        T2,
        T,
    )); // 4: deleted.txt (unallocated, dtime set)
    table.extend_from_slice(&inode_rec(
        0x41ED,
        4096,
        2,
        0x80000,
        &extent_root(&[(0, 12, 1)]),
        T,
        T,
        T,
        0,
        T,
    )); // 5: sub
    table.extend_from_slice(&inode_rec(
        0x81A4,
        8192,
        1,
        0x80000,
        &extent_root(&[(0, 22, 1), (1, 24, 1)]),
        T,
        T,
        T,
        0,
        T,
    )); // 6: inner.bin (fragmented: 22 then 24)
    // inodes 7..14 (idx 6..13): zero records
    for _ in 7..15 {
        table.extend_from_slice(&[0u8; 256]);
    }
    table.extend_from_slice(&inode_rec(
        0x81A4,
        8,
        0,
        0x80000,
        &extent_root(&[(0, 26, 1)]),
        T,
        T,
        T,
        T2,
        T,
    )); // 15: orphan (no dirent names it)
    table.extend_from_slice(&[0u8; 256]); // inode 16: zero
    img[4 * BS..4 * BS + table.len()].copy_from_slice(&table);

    // --- root dir at block 10: ".", "..", "sub", "hello.txt"(+shadow) ---
    let mut root = Vec::new();
    root.extend_from_slice(&dirent(2, ".", 12, 2));
    root.extend_from_slice(&dirent(2, "..", 12, 2));
    root.extend_from_slice(&dirent(5, "sub", 12, 2));
    // hello.txt: rec_len 64 covering itself + shadowed deleted.txt at +32
    let mut hello = dirent(3, "hello.txt", 64, 1);
    let shadow = dirent(4, "deleted.txt", 32, 1);
    hello[32..32 + shadow.len()].copy_from_slice(&shadow);
    root.extend_from_slice(&hello);
    img[10 * BS..10 * BS + root.len()].copy_from_slice(&root);

    // --- sub dir at block 12: ".", "..", "inner.bin" ---
    let mut sub = Vec::new();
    sub.extend_from_slice(&dirent(5, ".", 12, 2));
    sub.extend_from_slice(&dirent(2, "..", 12, 2));
    sub.extend_from_slice(&dirent(6, "inner.bin", 20, 1)); // 8+9=17 -> 20 (4-align)
    img[12 * BS..12 * BS + sub.len()].copy_from_slice(&sub);

    // --- file contents ---
    img[11 * BS..11 * BS + 12].copy_from_slice(b"hello world\n");
    img[20 * BS..20 * BS + 20].copy_from_slice(b"deleted data here!!\0");
    img[22 * BS..23 * BS].fill(0xAA);
    img[24 * BS..25 * BS].fill(0xBB);
    img[26 * BS..26 * BS + 8].fill(0xCC);

    // --- a valid JPEG placed only in free blocks (30..) ---
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
    img[30 * BS..30 * BS + jpg.len()].copy_from_slice(&jpg);

    img
}

async fn scan_image(mode: ScanMode) -> Arc<ResultsStore> {
    let img = build_image();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &img).unwrap();
    let source: Arc<dyn DiskSource> = Arc::new(FileBackedSource::open(tmp.path()).unwrap());
    let results = ResultsStore::new(10_000, None);
    let cancel = Arc::new(AtomicBool::new(false));
    ExtPlugin
        .scan(source, mode, results.clone(), no_progress(), cancel)
        .await
        .expect("ext scan");
    results
}

fn find(store: &ResultsStore, name: &str) -> Vec<dw_core::results::RecoveredEntry> {
    store
        .page(0, 10_000)
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

// ---- tests ---------------------------------------------------------------

#[tokio::test]
async fn probe_classifies_ext4() {
    let img = build_image();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &img).unwrap();
    let source: Arc<dyn DiskSource> = Arc::new(FileBackedSource::open(tmp.path()).unwrap());
    let out = ExtPlugin.probe(source.as_ref()).await.unwrap();
    match out {
        ProbeOutcome::Yes(p) => {
            assert_eq!(p.fs_kind, "ext4");
            assert_eq!(p.volume_label.as_deref(), Some("TESTEXT4"));
            assert_eq!(p.total_bytes, 1_048_576);
            assert_eq!(p.cluster_size, 4096);
        }
        other => panic!("expected Yes, got {other:?}"),
    }
}

#[tokio::test]
async fn quick_scan_finds_live_files_in_logical_order() {
    let store = scan_image(ScanMode::Quick).await;
    let inner = find(&store, "inner.bin");
    assert_eq!(inner.len(), 1);
    let e = &inner[0];
    assert!(!e.was_deleted);
    assert_eq!(e.size, 8192);
    assert_eq!(
        e.extents,
        vec![
            Extent {
                start_byte: 22 * 4096,
                length: 4096
            },
            Extent {
                start_byte: 24 * 4096,
                length: 4096
            },
        ]
    );
    assert_eq!(e.status, RecoveryStatus::Intact);
    assert_eq!(e.parent_path, "/sub");
    assert_eq!(e.mtime.map(|t| t.unix_timestamp()), Some(i64::from(T)));
    // crtime surfaced as ctime (ext4 records creation time).
    assert_eq!(e.ctime.map(|t| t.unix_timestamp()), Some(i64::from(T)));
}

#[tokio::test]
async fn quick_scan_hello_txt_extent_trimmed_to_size() {
    let store = scan_image(ScanMode::Quick).await;
    let e = &find(&store, "hello.txt")[0];
    assert_eq!(e.size, 12);
    assert_eq!(
        e.extents,
        vec![Extent {
            start_byte: 11 * 4096,
            length: 12
        }]
    );
}

#[tokio::test]
async fn quick_scan_shadow_dirent_names_deleted_file() {
    let store = scan_image(ScanMode::Quick).await;
    let del = find(&store, "deleted.txt");
    assert_eq!(del.len(), 1);
    let e = &del[0];
    assert!(e.was_deleted);
    assert_eq!(e.size, 20);
    assert_eq!(
        e.extents,
        vec![Extent {
            start_byte: 20 * 4096,
            length: 20
        }]
    );
    assert_eq!(e.status, RecoveryStatus::Intact);
    assert!(e.confidence >= 900);
}

#[tokio::test]
async fn quick_mode_runs_no_orphan_or_carve_pass() {
    let store = scan_image(ScanMode::Quick).await;
    assert!(
        find(&store, "inode_15").is_empty(),
        "orphan pass must not run in quick"
    );
    let all = store.page(0, 10_000);
    assert!(
        all.iter().all(|e| e.extension != "jpg"),
        "carving must not run in quick"
    );
    assert_eq!(all.len(), 3); // hello.txt, inner.bin, deleted.txt
}

#[tokio::test]
async fn deep_scan_orphan_pass_surfaces_unnamed_inode() {
    let store = scan_image(ScanMode::Deep).await;
    let orph = find(&store, "inode_15");
    assert_eq!(orph.len(), 1);
    let e = &orph[0];
    assert!(e.was_deleted);
    assert_eq!(e.size, 8);
    assert_eq!(
        e.extents,
        vec![Extent {
            start_byte: 26 * 4096,
            length: 8
        }]
    );
}

#[tokio::test]
async fn deep_scan_carves_jpeg_from_free_blocks_only() {
    let store = scan_image(ScanMode::Deep).await;
    let jpgs: Vec<_> = store
        .page(0, 10_000)
        .into_iter()
        .filter(|e| e.extension == "jpg")
        .collect();
    assert!(!jpgs.is_empty(), "free-block carving found no JPEG");
    for j in &jpgs {
        assert!(j.extents.iter().all(|x| x.start_byte >= 30 * 4096));
        assert_eq!(j.status, RecoveryStatus::CarvedValidated);
    }
}

#[tokio::test]
async fn deep_scan_does_not_duplicate_live_entries() {
    let store = scan_image(ScanMode::Deep).await;
    let all = store.page(0, 10_000);
    for name in ["hello.txt", "inner.bin", "deleted.txt", "inode_15"] {
        let n = all.iter().filter(|e| e.name == name).count();
        assert_eq!(n, 1, "{name} emitted {n} times");
    }
}
