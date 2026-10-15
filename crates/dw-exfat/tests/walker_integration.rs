//! End-to-end walker test over a synthetic exFAT image.
//!
//! Geometry: 512 B sectors, 4096 B clusters (8 sectors/cluster).
//! FatOffset = sector 128, FatLength = 16 sectors, ClusterHeapOffset =
//! sector 256, ClusterCount = 64, root = cluster 2.
//!
//! Cluster map (cluster N at 131072 + (N-2)*4096):
//!   2  root directory
//!   3  directory "docs" (in-use, FAT-chained: 3 -> EOC)
//!   4,5   "hello.txt"  in-use, FAT-chained (4 -> 5 -> EOC), 6000 bytes
//!   6     "photo.bin"  in-use, NoFatChain, 1000 bytes
//!   7     "gone.txt"   DELETED, NoFatChain, 500 bytes  -> fully recoverable
//!   8     "lost.dat"   DELETED, FAT-chained (FAT entry zeroed), 9000 bytes
//!                     -> PartiallyRecovered (only first cluster known)
//!   9     allocation bitmap chain (8 bytes)
//!   10    "docs/note.md" in-use, NoFatChain, 200 bytes
//!   11..65 free

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use dw_core::{ProgressEvent, ProgressSink, RecoveryStatus, ResultsStore};
use dw_disk::{DiskSource, FileBackedSource};
use dw_exfat::bitmap::BitmapEntry;
use dw_exfat::boot::ExfatBoot;
use dw_exfat::walker::{self, WalkOutcome};

const CLUSTER_HEAP: u64 = 131_072; // sector 256
const CLUSTER_SIZE: u64 = 4096;
const FAT_OFFSET: u64 = 65_536; // sector 128
const IMAGE_SIZE: usize = 524_288; // 1024 sectors

fn cluster_byte(c: u32) -> u64 {
    CLUSTER_HEAP + u64::from(c - 2) * CLUSTER_SIZE
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

/// FAT date/time bitfields -> u32 timestamp (date high 16, time low 16).
fn ts(date: u16, time: u16) -> u32 {
    (u32::from(date) << 16) | u32::from(time)
}

const DATE_2021_06_15: u16 = (41 << 9) | (6 << 5) | 15; // 2021-06-15
const TIME_1430_20: u16 = (14 << 11) | (30 << 5) | 10; // 14:30:20 [skip]
const DATE_2022_01_01: u16 = (42 << 9) | (1 << 5) | 1; // 2022-01-01
const TIME_1000_10: u16 = (10 << 11) | 5; // 10:00:10 (min=0)
const DATE_2022_01_02: u16 = (42 << 9) | (1 << 5) | 2; // 2022-01-02
const TIME_1200_04: u16 = (12 << 11) | 2; // 12:00:04 (min=0)

fn put_entry_set(
    img: &mut [u8],
    at: usize,
    in_use: bool,
    attrs: u16,
    is_stream_nofatchain: bool,
    first_cluster: u32,
    data_len: u64,
    name: &str,
    create_ts: u32,
    modified_ts: u32,
    accessed_ts: u32,
    create_10ms: u8,
) {
    let primary_type: u8 = if in_use { 0x85 } else { 0x05 };
    let secondary_type: u8 = if in_use { 0xC0 } else { 0x40 };
    let filename_type: u8 = if in_use { 0xC1 } else { 0x41 };

    // Primary @ at
    img[at] = primary_type;
    img[at + 1] = 2; // SecondaryCount: stream + 1 filename
    img[at + 2..at + 4].copy_from_slice(&0u16.to_le_bytes()); // SetChecksum (not validated)
    img[at + 4..at + 6].copy_from_slice(&attrs.to_le_bytes());
    // abs 8..20 timestamps
    img[at + 8..at + 12].copy_from_slice(&create_ts.to_le_bytes());
    img[at + 12..at + 16].copy_from_slice(&modified_ts.to_le_bytes());
    img[at + 16..at + 20].copy_from_slice(&accessed_ts.to_le_bytes());
    img[at + 20] = create_10ms;

    // Stream @ at+32 (spec: 1 flags, 2 Reserved1, 3 NameLength, 4..6 hash,
    // 8..16 ValidDataLength, 20..24 FirstCluster, 24..32 DataLength)
    let s = at + 32;
    img[s] = secondary_type;
    img[s + 1] = if is_stream_nofatchain { 0x03 } else { 0x01 };
    img[s + 3] = u8::try_from(name.encode_utf16().count()).unwrap_or(255);
    img[s + 8..s + 16].copy_from_slice(&data_len.to_le_bytes());
    img[s + 20..s + 24].copy_from_slice(&first_cluster.to_le_bytes());
    img[s + 24..s + 32].copy_from_slice(&data_len.to_le_bytes());

    // Filename @ at+64 (abs 2..32: 30 bytes UTF-16LE)
    let f = at + 64;
    img[f] = filename_type;
    let units: Vec<u16> = name.encode_utf16().collect();
    for (i, &u) in units.iter().take(15).enumerate() {
        img[f + 2 + i * 2] = (u & 0xFF) as u8;
        img[f + 3 + i * 2] = (u >> 8) as u8;
    }
    // terminator
    let term = f + 2 + units.len() * 2;
    if term + 1 < f + 32 {
        img[term] = 0;
        img[term + 1] = 0;
    }
}

fn build_image() -> Vec<u8> {
    let mut img = vec![0u8; IMAGE_SIZE];

    // ---- Boot sector ----
    let mut boot = [0u8; 512];
    boot[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    boot[64..72].copy_from_slice(&0u64.to_le_bytes()); // PartitionOffset
    boot[72..80].copy_from_slice(&1024u64.to_le_bytes()); // VolumeLength
    boot[80..84].copy_from_slice(&128u32.to_le_bytes()); // FatOffset
    boot[84..88].copy_from_slice(&16u32.to_le_bytes()); // FatLength
    boot[88..92].copy_from_slice(&256u32.to_le_bytes()); // ClusterHeapOffset
    boot[92..96].copy_from_slice(&64u32.to_le_bytes()); // ClusterCount
    boot[96..100].copy_from_slice(&2u32.to_le_bytes()); // RootDirFirstCluster
    boot[100..104].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // Serial
    boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes()); // Revision
    boot[108] = 9; // BytesPerSectorShift (512)
    boot[109] = 3; // SectorsPerClusterShift (8 -> 4096)
    boot[110] = 1; // NumberOfFats
    boot[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());
    img[0..512].copy_from_slice(&boot);

    // ---- FAT ----
    let fat = &mut img[FAT_OFFSET as usize..(FAT_OFFSET + 8192) as usize];
    let entry = |fat: &mut [u8], idx: usize, v: u32| {
        fat[idx * 4..idx * 4 + 4].copy_from_slice(&v.to_le_bytes());
    };
    entry(fat, 0, 0xFFFF_FFF8);
    entry(fat, 1, 0xFFFF_FFFF);
    entry(fat, 2, 0xFFFF_FFFF); // root: single cluster
    entry(fat, 3, 0xFFFF_FFFF); // docs: single cluster
    entry(fat, 4, 5); // hello.txt: 4 -> 5
    entry(fat, 5, 0xFFFF_FFFF);
    entry(fat, 6, 0xFFFF_FFFF); // photo.bin NoFatChain (FAT unused)
    entry(fat, 7, 0xFFFF_FFFF); // gone.txt NoFatChain
    entry(fat, 8, 0); // lost.dat deleted: zeroed
    entry(fat, 9, 0xFFFF_FFFF); // bitmap chain end
    entry(fat, 10, 0xFFFF_FFFF); // note.md NoFatChain

    // ---- Root directory (cluster 2) ----
    let root = cluster_byte(2) as usize;
    // Allocation Bitmap entry @0: first_cluster 9, length 8
    img[root] = 0x81;
    img[root + 5..root + 9].copy_from_slice(&9u32.to_le_bytes());
    img[root + 9..root + 17].copy_from_slice(&8u64.to_le_bytes());
    // Volume label @32: "TESTVOL"
    let vl = root + 32;
    img[vl] = 0x83;
    let label: Vec<u16> = "TESTVOL".encode_utf16().collect();
    for (i, &u) in label.iter().enumerate() {
        img[vl + 2 + i * 2] = (u & 0xFF) as u8;
        img[vl + 3 + i * 2] = (u >> 8) as u8;
    }
    // hello.txt (in-use, FAT-chained) @64
    put_entry_set(
        &mut img,
        root + 64,
        true,
        0x0020,
        false,
        4,
        6000,
        "hello.txt",
        ts(DATE_2021_06_15, TIME_1430_20),
        ts(DATE_2022_01_01, TIME_1000_10),
        ts(DATE_2022_01_02, TIME_1200_04),
        137,
    );
    // photo.bin (in-use, NoFatChain) @160
    put_entry_set(
        &mut img,
        root + 160,
        true,
        0x0020,
        true,
        6,
        1000,
        "photo.bin",
        0,
        0,
        0,
        0,
    );
    // gone.txt (deleted, NoFatChain) @256
    put_entry_set(
        &mut img,
        root + 256,
        false,
        0x0020,
        true,
        7,
        500,
        "gone.txt",
        0,
        0,
        0,
        0,
    );
    // lost.dat (deleted, FAT-chained) @352
    put_entry_set(
        &mut img,
        root + 352,
        false,
        0x0020,
        false,
        8,
        9000,
        "lost.dat",
        0,
        0,
        0,
        0,
    );
    // docs directory (in-use) @448
    put_entry_set(
        &mut img,
        root + 448,
        true,
        0x0010,
        false,
        3,
        0,
        "docs",
        0,
        0,
        0,
        0,
    );
    // End of directory @544
    img[root + 544] = 0x00;

    // ---- "docs" directory (cluster 3) ----
    let docs = cluster_byte(3) as usize;
    put_entry_set(
        &mut img, docs, true, 0x0020, true, 10, 200, "note.md", 0, 0, 0, 0,
    );
    img[docs + 96] = 0x00; // end of directory

    // ---- Bitmap data (cluster 9): 64 clusters -> 8 bytes.
    // Allocated: clusters 2..10 (bits 0..8) -> byte0 = 0xFF, byte1 bit0 = 1.
    let bm = cluster_byte(9) as usize;
    img[bm] = 0xFF;
    img[bm + 1] = 0x01;

    img
}

async fn run_walk() -> (WalkOutcome, std::sync::Arc<ResultsStore>) {
    let img = build_image();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &img).unwrap();
    let source: Arc<dyn DiskSource> = Arc::new(FileBackedSource::open(tmp.path()).unwrap());

    let mut boot_buf = [0u8; 512];
    boot_buf.copy_from_slice(&img[0..512]);
    let boot = ExfatBoot::parse(&boot_buf).unwrap();
    let fat = walker::read_fat(source.as_ref(), &boot).unwrap();

    let results = ResultsStore::new(1000, None);
    let progress = no_progress();
    let outcome = walker::walk_exfat(source, &boot, &fat, results.clone(), progress)
        .await
        .unwrap();
    (outcome, results)
}

#[tokio::test]
async fn walks_synthetic_exfat_image() {
    let (outcome, results) = run_walk().await;

    // ---- Outcome ----
    assert_eq!(outcome.entries, 6, "docs + note.md + 4 root files");
    assert_eq!(outcome.volume_label.as_deref(), Some("TESTVOL"));
    let bm_entry: BitmapEntry = outcome.bitmap.expect("bitmap entry found");
    assert_eq!(bm_entry.first_cluster, 9);
    assert_eq!(bm_entry.length, 8);

    // ---- Entries ----
    let all = results.snapshot();
    let find = |name: &str| {
        all.iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("entry {name} missing"))
    };

    // hello.txt: FAT-chained, intact, single merged extent, full timestamps.
    let hello = find("hello.txt");
    assert_eq!(hello.size, 6000);
    assert!(!hello.was_deleted);
    assert_eq!(hello.status, RecoveryStatus::Intact);
    assert_eq!(
        hello.extents,
        vec![dw_core::Extent {
            start_byte: cluster_byte(4),
            length: 6000
        }]
    );
    assert_eq!(hello.cluster_chain, vec![4, 5]);
    assert_eq!(hello.parent_path, "\\");
    // ctime: 2021-06-15 14:30:20 + 137*10ms = 14:30:21.370
    let ctime = hello.ctime.expect("ctime");
    assert_eq!(
        (ctime.year(), u8::from(ctime.month()), ctime.day()),
        (2021, 6, 15)
    );
    assert_eq!((ctime.hour(), ctime.minute(), ctime.second()), (14, 30, 21));
    assert_eq!(ctime.millisecond(), 370);
    // mtime: 2022-01-01 10:00:10
    let mtime = hello.mtime.expect("mtime");
    assert_eq!(
        (mtime.year(), u8::from(mtime.month()), mtime.day()),
        (2022, 1, 1)
    );
    assert_eq!((mtime.hour(), mtime.minute(), mtime.second()), (10, 0, 10));
    // atime: 2022-01-02 12:00:04
    let atime = hello.atime.expect("atime");
    assert_eq!(
        (atime.year(), u8::from(atime.month()), atime.day()),
        (2022, 1, 2)
    );
    assert_eq!((atime.hour(), atime.minute(), atime.second()), (12, 0, 4));

    // photo.bin: NoFatChain in-use.
    let photo = find("photo.bin");
    assert_eq!(photo.status, RecoveryStatus::Intact);
    assert_eq!(photo.extents.len(), 1);
    assert_eq!(photo.extents[0].start_byte, cluster_byte(6));
    assert_eq!(photo.extents[0].length, 1000);

    // gone.txt: DELETED + NoFatChain -> still fully recoverable.
    let gone = find("gone.txt");
    assert!(gone.was_deleted);
    assert_eq!(gone.status, RecoveryStatus::Intact);
    assert_eq!(
        gone.extents,
        vec![dw_core::Extent {
            start_byte: cluster_byte(7),
            length: 500
        }]
    );

    // lost.dat: DELETED + FAT-chained (FAT zeroed) -> partial.
    let lost = find("lost.dat");
    assert!(lost.was_deleted);
    assert_eq!(lost.status, RecoveryStatus::PartiallyRecovered);
    assert_eq!(lost.size, 9000);
    assert_eq!(
        lost.extents,
        vec![dw_core::Extent {
            start_byte: cluster_byte(8),
            length: 4096
        }]
    );

    // docs: directory, size 0, recursed into.
    let docs_entry = find("docs");
    assert_eq!(docs_entry.size, 0);
    assert_eq!(docs_entry.parent_path, "\\");

    // note.md: inside docs.
    let note = find("note.md");
    assert_eq!(note.parent_path, "\\docs");
    assert_eq!(note.status, RecoveryStatus::Intact);
    assert_eq!(
        note.extents,
        vec![dw_core::Extent {
            start_byte: cluster_byte(10),
            length: 200
        }]
    );
}

#[tokio::test]
async fn bitmap_free_regions_match_expected_holes() {
    // Deep-scan companion: the walk returns the bitmap entry; reading the
    // bitmap data and computing free runs must yield exactly the free
    // clusters (11..65) as one run.
    let img = build_image();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &img).unwrap();
    let source: Arc<dyn DiskSource> = Arc::new(FileBackedSource::open(tmp.path()).unwrap());

    let mut boot_buf = [0u8; 512];
    boot_buf.copy_from_slice(&img[0..512]);
    let boot = ExfatBoot::parse(&boot_buf).unwrap();
    let fat = walker::read_fat(source.as_ref(), &boot).unwrap();

    let results = ResultsStore::new(1000, None);
    let progress = no_progress();
    let outcome = walker::walk_exfat(source.clone(), &boot, &fat, results, progress)
        .await
        .unwrap();

    let bm_entry = outcome.bitmap.expect("bitmap entry");
    let read_at = source.as_read_at();
    let data = walker::read_bitmap_data(&read_at, &boot, &fat, &bm_entry).unwrap();
    let bitmap = dw_exfat::bitmap::ExfatBitmap::new(data, boot.cluster_count).unwrap();
    let runs = bitmap.free_runs();
    assert_eq!(
        runs,
        vec![(11u32, 55u32)],
        "clusters 11..65 free as one run"
    );
}
