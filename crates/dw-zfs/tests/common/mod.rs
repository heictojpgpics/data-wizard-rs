//! Synthetic OpenZFS pool fixture builder (test-only).
//!
//! Two uberblock generations:
//! - txg 10 (old): tree contains hello.txt, sub/, sub/inner.bin
//! - txg 11 (new): sub/inner.bin removed (entry gone from the sub zap,
//!   object slot freed) - the deep pass must recover it from txg 10
//!
//! Formats exercised: microzap (master nodes, registry, sub dir, MOS zap),
//! fatzap (root dir with 16 KiB blocks; layouts zap with 512 B blocks and
//! u16-array values), LZ4 (ZFS 4-byte BE header, sector-padded) with
//! fletcher4, uncompressed fletcher2, XDR nvlists with nested lists and
//! nvlist arrays, and the 4-label geometry.

#![allow(dead_code)]

use dw_zfs::checksum;
use std::cell::RefCell;

pub const POOL_SIZE: u64 = 64 * 1024 * 1024;
/// Data region base: everything lives in [8 MiB, 9 MiB).
pub const BASE: u64 = 8 * 1024 * 1024;

// Old generation (txg 10)
pub const OFF_MOS_OLD: u64 = BASE;
pub const OFF_MOS_DNODES_OLD: u64 = BASE + 20 * 1024;
pub const OFF_MOS_ZAP_OLD: u64 = BASE + 36 * 1024;
pub const OFF_FS_OLD: u64 = BASE + 40 * 1024;
pub const OFF_FS_DNODES_OLD: u64 = BASE + 44 * 1024;
pub const OFF_ROOTDIR_ZAP_OLD: u64 = BASE + 60 * 1024; // fatzap hdr (16 KiB)
pub const OFF_LEAF1_OLD: u64 = BASE + 76 * 1024; // fatzap leaf (16 KiB)
pub const OFF_SUBDIR_ZAP_OLD: u64 = BASE + 92 * 1024;
pub const OFF_SA_MASTER_OLD: u64 = BASE + 96 * 1024;
pub const OFF_REGISTRY_OLD: u64 = BASE + 100 * 1024;
pub const OFF_LAYOUTS_OLD: u64 = BASE + 104 * 1024; // fatzap hdr 512 B
pub const OFF_LAYOUTS_LEAF_OLD: u64 = BASE + 104 * 1024 + 512;
pub const OFF_HELLO_DATA: u64 = BASE + 112 * 1024; // LZ4, 512 B psize
pub const OFF_INNER_DATA: u64 = BASE + 120 * 1024; // 4 KiB, fletcher2
pub const OFF_FS_MASTER_OLD: u64 = BASE + 116 * 1024 + 512;

// New generation (txg 11)
pub const OFF_MOS_NEW: u64 = BASE + 256 * 1024;
pub const OFF_MOS_DNODES_NEW: u64 = BASE + 260 * 1024;
pub const OFF_MOS_ZAP_NEW: u64 = BASE + 276 * 1024;
pub const OFF_FS_NEW: u64 = BASE + 280 * 1024;
pub const OFF_FS_DNODES_NEW: u64 = BASE + 284 * 1024;
pub const OFF_ROOTDIR_ZAP_NEW: u64 = BASE + 300 * 1024;
pub const OFF_LEAF1_NEW: u64 = BASE + 316 * 1024;
pub const OFF_SUBDIR_ZAP_NEW: u64 = BASE + 332 * 1024;
pub const OFF_FS_MASTER_NEW: u64 = BASE + 336 * 1024;

pub const TXG_OLD: u64 = 10;
pub const TXG_NEW: u64 = 11;
pub const HELLO_SIZE: u64 = 12;
pub const HELLO_MTIME: u64 = 1_700_000_000;
pub const INNER_MTIME: u64 = 1_600_000_000;

// ---------------------------------------------------------------------------
// XDR nvlist encoding (writer side of the parser in src/nvlist.rs)
// ---------------------------------------------------------------------------

fn x_u32(n: &mut Vec<u8>, v: u32) {
    n.extend_from_slice(&v.to_le_bytes());
}
fn x_u64(n: &mut Vec<u8>, v: u64) {
    n.extend_from_slice(&v.to_le_bytes());
}
fn x_str(n: &mut Vec<u8>, s: &str) {
    // XDR string: u32 non-NUL length, data, pad to NV_ALIGN4(len). The
    // trailing NUL is part of the padding, never counted.
    let b = s.as_bytes();
    x_u32(n, b.len() as u32);
    let before = n.len();
    n.extend_from_slice(b);
    while !(n.len() - before).is_multiple_of(4) {
        n.push(0);
    }
}

fn nv_header(n: &mut Vec<u8>) {
    x_u32(n, 1); // method: XDR
    x_u32(n, 0); // version
    x_u32(n, 0); // nvflag
}

fn nv_end(n: &mut Vec<u8>) {
    x_u32(n, 0);
    x_u32(n, 0);
}

fn nv_pair_str(n: &mut Vec<u8>, name: &str, value: &str) {
    let mut body = Vec::new();
    x_str(&mut body, name);
    x_u32(&mut body, 9); // DATA_TYPE_STRING
    x_u32(&mut body, 1);
    x_str(&mut body, value);
    x_u32(n, 8 + body.len() as u32);
    x_u32(n, 24 + body.len() as u32);
    n.extend_from_slice(&body);
}

fn nv_pair_u64(n: &mut Vec<u8>, name: &str, value: u64) {
    let mut body = Vec::new();
    x_str(&mut body, name);
    x_u32(&mut body, 8); // DATA_TYPE_UINT64
    x_u32(&mut body, 1);
    x_u64(&mut body, value);
    x_u32(n, 8 + body.len() as u32);
    x_u32(n, 24 + body.len() as u32);
    n.extend_from_slice(&body);
}

/// A pair whose value is a nested nvlist (full stream incl. its header).
fn nv_pair_nvlist(n: &mut Vec<u8>, name: &str, nested: &[u8]) {
    let mut body = Vec::new();
    x_str(&mut body, name);
    x_u32(&mut body, 19); // DATA_TYPE_NVLIST
    x_u32(&mut body, 1);
    body.extend_from_slice(nested);
    x_u32(n, 8 + body.len() as u32);
    x_u32(n, 24 + body.len() as u32);
    n.extend_from_slice(&body);
}

/// A pair whose value is an nvlist array (back-to-back full streams).
fn nv_pair_nvlist_array(n: &mut Vec<u8>, name: &str, items: &[Vec<u8>]) {
    let mut body = Vec::new();
    x_str(&mut body, name);
    x_u32(&mut body, 20); // DATA_TYPE_NVLIST_ARRAY
    x_u32(&mut body, items.len() as u32);
    for it in items {
        body.extend_from_slice(it);
    }
    x_u32(n, 8 + body.len() as u32);
    x_u32(n, 24 + body.len() as u32);
    n.extend_from_slice(&body);
}

/// The label nvlist body (no eck): pool config for a single-disk pool.
fn label_nvlist_body() -> Vec<u8> {
    // child vdev: {type: disk, guid: ...}
    let mut child = Vec::new();
    nv_header(&mut child);
    nv_pair_str(&mut child, "type", "disk");
    nv_pair_u64(&mut child, "guid", 0xABCD_1234);
    nv_end(&mut child);

    // vdev_tree: {type: root, ashift: 9, children: [child]}
    let mut tree = Vec::new();
    nv_header(&mut tree);
    nv_pair_str(&mut tree, "type", "root");
    nv_pair_u64(&mut tree, "ashift", 9);
    nv_pair_nvlist_array(&mut tree, "children", &[child]);
    nv_end(&mut tree);

    let mut n = Vec::new();
    nv_header(&mut n);
    nv_pair_str(&mut n, "name", "testpool");
    nv_pair_u64(&mut n, "version", 5000);
    nv_pair_u64(&mut n, "pool_guid", 0x1234_0000_5678);
    nv_pair_nvlist(&mut n, "vdev_tree", &tree);
    nv_end(&mut n);
    n
}

/// Full vdev_phys region (112 KiB): nvlist, zero pad, zio_eck with a
/// fletcher4 seal over the region minus the eck.
fn build_vdev_phys() -> Vec<u8> {
    let body = label_nvlist_body();
    let mut region = vec![0u8; 112 * 1024];
    region[..body.len()].copy_from_slice(&body);
    let c = checksum::label_fletcher4(&region);
    let eck_at = region.len() - 40;
    region[eck_at + 8..eck_at + 40].copy_from_slice(&c.0.map(u64::to_le_bytes).concat()[..32]);
    region
}

// ---------------------------------------------------------------------------
// blkptr helpers
// ---------------------------------------------------------------------------

fn dva_words(offset: u64, asize: u64) -> [u8; 16] {
    let mut w = [0u8; 16];
    w[0..8].copy_from_slice(&((asize / 512) & 0xff_ffff).to_le_bytes());
    w[8..16].copy_from_slice(&(offset / 512).to_le_bytes());
    w
}

fn prop_word(lsize: u64, psize: u64, comp: u8, cksum_kind: u8, otype: u8, level: u8) -> u64 {
    let lsize_bits = ((lsize / 512) - 1) & 0xffff;
    let psize_bits = if psize == 0 {
        0xffff
    } else {
        ((psize / 512) - 1) & 0xffff
    };
    lsize_bits
        | (psize_bits << 16)
        | (u64::from(comp) << 32)
        | (u64::from(cksum_kind) << 40)
        | (u64::from(otype) << 48)
        | (u64::from(level) << 56)
}

/// 128-byte blkptr with the cksum sealed over `data` (the on-disk bytes).
fn blkptr(
    offset: u64,
    asize: u64,
    lsize: u64,
    psize: u64,
    comp: u8,
    cksum_kind: u8,
    otype: u8,
    level: u8,
    data: &[u8],
) -> [u8; 128] {
    let mut bp = [0u8; 128];
    bp[0..16].copy_from_slice(&dva_words(offset, asize));
    bp[48..56]
        .copy_from_slice(&prop_word(lsize, psize, comp, cksum_kind, otype, level).to_le_bytes());
    let c = match cksum_kind {
        6 => checksum::fletcher2(data),
        _ => checksum::fletcher4(data, false),
    };
    bp[96..128].copy_from_slice(&c.0.map(u64::to_le_bytes).concat()[..32]);
    bp
}

// ---------------------------------------------------------------------------
// dnode / objset
// ---------------------------------------------------------------------------

pub struct DnodeSpec {
    pub type_: u8,
    pub bonustype: u8,
    pub bonus: Vec<u8>,
    pub blkptr0: [u8; 128],
    pub blkptr1: Option<[u8; 128]>,
    pub nblkptr: u8,
    pub datablkszsec: u16,
    pub maxblkid: u64,
}

pub fn dnode_bytes(type_: u8, bonustype: u8) -> DnodeSpec {
    DnodeSpec {
        type_,
        bonustype,
        bonus: Vec::new(),
        blkptr0: [0u8; 128],
        blkptr1: None,
        nblkptr: 1,
        datablkszsec: 8, // 4 KiB data blocks by default
        maxblkid: 0,
    }
}

impl DnodeSpec {
    pub fn finish(&self) -> Vec<u8> {
        let mut dn = vec![0u8; 512];
        dn[0] = self.type_;
        dn[1] = 17; // indblkshift (unused at nlevels 1)
        dn[2] = 1; // nlevels
        dn[3] = self.nblkptr;
        dn[4] = self.bonustype;
        dn[8..10].copy_from_slice(&self.datablkszsec.to_le_bytes());
        let bl = self.bonus.len().min(320);
        dn[10..12].copy_from_slice(&(bl as u16).to_le_bytes());
        dn[16..24].copy_from_slice(&self.maxblkid.to_le_bytes());
        if !self.bonus.is_empty() {
            dn[192..192 + bl].copy_from_slice(&self.bonus[..bl]);
        }
        dn[64..192].copy_from_slice(&self.blkptr0);
        if let Some(bp1) = self.blkptr1 {
            dn[192..320].copy_from_slice(&bp1);
        }
        dn
    }
}

/// 4096-byte objset whose meta dnode (offset 0) points at `meta_bp`.
fn build_objset(meta_bp: &[u8; 128], meta_datablkszsec: u16) -> Vec<u8> {
    let mut os = vec![0u8; 4096];
    // meta dnode @0: type 10 (DNODE), nblkptr 1, data = dnode array
    os[0] = 10;
    os[2] = 1; // nlevels
    os[3] = 1; // nblkptr
    os[8..10].copy_from_slice(&meta_datablkszsec.to_le_bytes());
    os[64..192].copy_from_slice(meta_bp);
    os
}

// ---------------------------------------------------------------------------
// zap builders
// ---------------------------------------------------------------------------

pub const ZBT_MICRO: u64 = (1u64 << 63) + 3;
pub const ZBT_HEADER: u64 = (1u64 << 63) + 1;
pub const ZBT_LEAF: u64 = 1u64 << 63;
pub const ZAP_MAGIC: u64 = 0x0002_F52A_B2AB;
pub const ZAP_LEAF_MAGIC: u32 = 0x2AB_1EAF;

/// 512-byte microzap block.
fn build_microzap(entries: &[(String, u64)]) -> Vec<u8> {
    let mut z = vec![0u8; 1024];
    z[0..8].copy_from_slice(&ZBT_MICRO.to_le_bytes());
    z[8..16].copy_from_slice(&0xDEAD_BEEFu64.to_le_bytes()); // salt
    z[16..24].copy_from_slice(&0u64.to_le_bytes()); // normflags
    let mut off = 64;
    for (name, value) in entries {
        let ent = &mut z[off..off + 64];
        ent[0..8].copy_from_slice(&value.to_le_bytes());
        ent[8..12].copy_from_slice(&0u32.to_le_bytes()); // cd
        ent[14..14 + name.len()].copy_from_slice(name.as_bytes());
        off += 64;
        assert!(off <= 1024, "microzap overflow");
    }
    z
}

/// One fatzap with `blocksize`-byte blocks: header block (data block 0)
/// with an embedded ptr table pointing every entry to leaf block 1, plus
/// the leaf block itself. Values are u64 (intlen 8).
fn build_fatzap_u64(entries: &[(String, u64)], blocksize: usize) -> (Vec<u8>, Vec<u8>) {
    build_fatzap(entries, blocksize, 8, |v| {
        let mut b = Vec::new();
        b.extend_from_slice(&v.to_le_bytes());
        b
    })
}

/// Fatzap variant whose leaf values are u16 arrays (intlen 2) - the
/// layouts zap shape.
fn build_fatzap_u16(entries: &[(String, Vec<u16>)], blocksize: usize) -> (Vec<u8>, Vec<u8>) {
    build_fatzap(entries, blocksize, 2, |arr| {
        let mut b = Vec::new();
        for v in arr {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    })
}

fn build_fatzap<T, F>(
    entries: &[(String, T)],
    blocksize: usize,
    intlen: u8,
    encode: F,
) -> (Vec<u8>, Vec<u8>)
where
    F: Fn(&T) -> Vec<u8>,
{
    assert!(blocksize.is_power_of_two());
    // --- leaf ---
    let hash_entries = 1usize << (blocksize.trailing_zeros() - 5);
    let chunks_base = 48 + hash_entries * 2;
    let num_chunks = (blocksize - chunks_base) / 24;
    let mut leaf = vec![0u8; blocksize];
    leaf[0..8].copy_from_slice(&ZBT_LEAF.to_le_bytes());
    leaf[24..28].copy_from_slice(&ZAP_LEAF_MAGIC.to_le_bytes());
    leaf[28..30].copy_from_slice(&(num_chunks as u16 - 1).to_le_bytes()); // nfree
    leaf[30..32].copy_from_slice(&(entries.len() as u16).to_le_bytes()); // nentries

    let mut chunk_idx = 0usize;
    let write_chunk = |leaf: &mut [u8], idx: usize, bytes: &[u8; 24]| {
        leaf[chunks_base + idx * 24..chunks_base + idx * 24 + 24].copy_from_slice(bytes);
    };
    for (name, value) in entries {
        // entry chunk
        let value_bytes = encode(value);
        let name_padded = {
            let mut b = name.as_bytes().to_vec();
            b.push(0);
            b
        };
        let name_chunks = name_padded.len().div_ceil(21);
        let value_chunks = value_bytes.len().div_ceil(21);
        let entry_idx = chunk_idx;
        let name_idx = entry_idx + 1;
        let value_idx = name_idx + name_chunks;
        chunk_idx = value_idx + value_chunks;
        assert!(chunk_idx < num_chunks, "fatzap leaf overflow");

        let mut e = [0u8; 24];
        e[0] = 252; // ZAP_CHUNK_ENTRY
        e[1] = intlen;
        // le_next: hash chains unused
        e[4..6].copy_from_slice(&(name_idx as u16).to_le_bytes());
        e[6..8].copy_from_slice(&(name_padded.len() as u16).to_le_bytes());
        e[8..10].copy_from_slice(&(value_idx as u16).to_le_bytes());
        // le_value_numints is the number of INTEGERS, not chunk count
        // (zap_leaf.h); for u64 singles these coincide, for u16 arrays
        // they do not.
        let value_numints = (value_bytes.len() / intlen.max(1) as usize).max(1);
        e[10..12].copy_from_slice(&(value_numints as u16).to_le_bytes());
        write_chunk(&mut leaf, entry_idx, &e);

        // name chunks (21 bytes payload, u16 la_next @22)
        for c in 0..name_chunks {
            let start = c * 21;
            let end = (start + 21).min(name_padded.len());
            let mut a = [0u8; 24];
            a[0] = 251; // ZAP_CHUNK_ARRAY
            a[1..1 + (end - start)].copy_from_slice(&name_padded[start..end]);
            let next = if c + 1 < name_chunks {
                (name_idx + c + 1) as u16
            } else {
                0xFFFF
            };
            a[22..24].copy_from_slice(&next.to_le_bytes());
            write_chunk(&mut leaf, name_idx + c, &a);
        }
        // value chunks
        for c in 0..value_chunks {
            let start = c * 21;
            let end = (start + 21).min(value_bytes.len());
            let mut a = [0u8; 24];
            a[0] = 251;
            a[1..1 + (end - start)].copy_from_slice(&value_bytes[start..end]);
            let next = if c + 1 < value_chunks {
                (value_idx + c + 1) as u16
            } else {
                0xFFFF
            };
            a[22..24].copy_from_slice(&next.to_le_bytes());
            write_chunk(&mut leaf, value_idx + c, &a);
        }
    }

    // --- header ---
    let mut hdr = vec![0u8; blocksize];
    let table_entries = entries.len().max(1).next_power_of_two();
    let zt_shift = table_entries.trailing_zeros();
    hdr[0..8].copy_from_slice(&ZBT_HEADER.to_le_bytes());
    hdr[8..16].copy_from_slice(&ZAP_MAGIC.to_le_bytes());
    // ptrtbl: zt_blk=0 (embedded)
    hdr[16..24].copy_from_slice(&0u64.to_le_bytes());
    hdr[24..32].copy_from_slice(&0u64.to_le_bytes()); // zt_numblks
    hdr[32..40].copy_from_slice(&u64::from(zt_shift).to_le_bytes());
    hdr[56..64].copy_from_slice(&1u64.to_le_bytes()); // zap_freeblk
    hdr[64..72].copy_from_slice(&1u64.to_le_bytes()); // zap_num_leafs
    hdr[72..80].copy_from_slice(&(entries.len() as u64).to_le_bytes());
    hdr[80..88].copy_from_slice(&0u64.to_le_bytes()); // salt
    // embedded table: every slot -> leaf block 1
    let half = blocksize / 2;
    for i in 0..table_entries {
        hdr[half + i * 8..half + i * 8 + 8].copy_from_slice(&1u64.to_le_bytes());
    }
    (hdr, leaf)
}

// ---------------------------------------------------------------------------
// SA bonus
// ---------------------------------------------------------------------------

fn sa_bonus(
    atime: u64,
    mtime: u64,
    size: u64,
    links: u64,
    parent: u64,
    generation: u64,
    mode: u64,
    uid: u64,
    gid: u64,
) -> Vec<u8> {
    let mut b = Vec::with_capacity(96);
    b.extend_from_slice(&0x2F_505Au32.to_le_bytes()); // SA_MAGIC
    b.extend_from_slice(&(1u16 | (1u16 << 10)).to_le_bytes()); // layout 1, hdr 8
    b.extend_from_slice(&0u16.to_le_bytes()); // unused length slot
    b.extend_from_slice(&atime.to_le_bytes()); // ATIME t0
    b.extend_from_slice(&mtime.to_le_bytes()); // ATIME t1
    b.extend_from_slice(&mtime.to_le_bytes()); // MTIME t0
    b.extend_from_slice(&atime.to_le_bytes()); // MTIME t1
    b.extend_from_slice(&size.to_le_bytes()); // SIZE
    b.extend_from_slice(&links.to_le_bytes()); // LINKS
    b.extend_from_slice(&parent.to_le_bytes()); // PARENT
    b.extend_from_slice(&generation.to_le_bytes()); // GEN
    b.extend_from_slice(&mode.to_le_bytes()); // MODE
    b.extend_from_slice(&uid.to_le_bytes()); // UID
    b.extend_from_slice(&gid.to_le_bytes()); // GID
    b
}

// ---------------------------------------------------------------------------
// whole-pool assembly
// ---------------------------------------------------------------------------

thread_local! {
    static ROOT_BPS: RefCell<(Vec<u8>, Vec<u8>)> =
        const { RefCell::new((Vec::new(), Vec::new())) };
}

fn build_uberblock(txg: u64, ts_seconds: u64) -> Vec<u8> {
    let bp = ROOT_BPS.with(|c| {
        let (old, new) = c.borrow().clone();
        if txg == TXG_NEW { new } else { old }
    });
    assert_eq!(bp.len(), 128, "rootbp not yet assembled");
    let mut ub = vec![0u8; 1024];
    let mut w = |o: usize, v: u64| ub[o..o + 8].copy_from_slice(&v.to_le_bytes());
    w(0, 0x00ba_b10c);
    w(8, 5000);
    w(16, txg);
    w(24, 0x1234_5678);
    w(32, ts_seconds);
    drop(w);
    ub[40..168].copy_from_slice(&bp);
    let mut w = |o: usize, v: u64| ub[o..o + 8].copy_from_slice(&v.to_le_bytes());
    w(168, 5000);
    w(200, 0);
    ub
}

pub fn build_pool() -> Vec<u8> {
    let mut img = vec![0u8; POOL_SIZE as usize];
    let put = |img: &mut [u8], off: u64, data: &[u8]| {
        img[off as usize..off as usize + data.len()].copy_from_slice(data);
    };

    // ---- shared content blocks (written once, referenced by both gens) --
    let hello_content = {
        let mut c = vec![0u8; 4096];
        c[..12].copy_from_slice(b"Hello world!");
        c
    };
    let (hello_disk, hello_psize) = {
        let block = lz4_flex::block::compress(&hello_content);
        let mut disk = Vec::with_capacity(512);
        disk.extend_from_slice(&(block.len() as u32).to_be_bytes());
        disk.extend_from_slice(&block);
        disk.resize(512, 0);
        (disk, 512u64)
    };
    let inner_content: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();

    let hello_bp = blkptr(
        OFF_HELLO_DATA,
        512,
        4096,
        hello_psize,
        15,
        7,
        19,
        0,
        &hello_disk,
    );
    put(&mut img, OFF_HELLO_DATA, &hello_disk);
    let inner_bp = blkptr(
        OFF_INNER_DATA,
        4096,
        4096,
        4096,
        2,
        6,
        19,
        0,
        &inner_content,
    );
    put(&mut img, OFF_INNER_DATA, &inner_content);

    // SA registry (microzap; u64 values)
    fn attr_word(num: u64, length: u64) -> u64 {
        (1u64 << 16) | (length << 24) | num
    }
    let registry_entries: Vec<(String, u64)> = vec![
        ("ZPL_ATIME".into(), attr_word(0, 16)),
        ("ZPL_MTIME".into(), attr_word(1, 16)),
        ("ZPL_SIZE".into(), attr_word(2, 8)),
        ("ZPL_LINKS".into(), attr_word(3, 8)),
        ("ZPL_PARENT".into(), attr_word(4, 8)),
        ("ZPL_GEN".into(), attr_word(5, 8)),
        ("ZPL_MODE".into(), attr_word(6, 8)),
        ("ZPL_UID".into(), attr_word(7, 8)),
        ("ZPL_GID".into(), attr_word(8, 8)),
    ];
    let registry_block = build_microzap(&registry_entries);
    let registry_bp = blkptr(
        OFF_REGISTRY_OLD,
        1024,
        1024,
        1024,
        2,
        7,
        46,
        0,
        &registry_block,
    );
    put(&mut img, OFF_REGISTRY_OLD, &registry_block);

    // SA layouts (fatzap, u16-array values, 512-byte blocks)
    let layout1: Vec<u16> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    let (layouts_hdr, layouts_leaf) = build_fatzap_u16(&[("1".to_string(), layout1)], 512);
    let layouts_bp = blkptr(OFF_LAYOUTS_OLD, 512, 512, 512, 2, 7, 47, 0, &layouts_hdr);
    let layouts_leaf_bp = blkptr(
        OFF_LAYOUTS_LEAF_OLD,
        512,
        512,
        512,
        2,
        7,
        47,
        0,
        &layouts_leaf,
    );
    put(&mut img, OFF_LAYOUTS_OLD, &layouts_hdr);
    put(&mut img, OFF_LAYOUTS_LEAF_OLD, &layouts_leaf);

    // SA master (microzap)
    let sa_master_block =
        build_microzap(&[("REGISTRY".to_string(), 6), ("LAYOUTS".to_string(), 8)]);
    let sa_master_bp = blkptr(
        OFF_SA_MASTER_OLD,
        1024,
        1024,
        1024,
        2,
        7,
        45,
        0,
        &sa_master_block,
    );
    put(&mut img, OFF_SA_MASTER_OLD, &sa_master_block);

    // ---- per generation ----
    for &new_gen in &[false, true] {
        let (off_mos, off_mos_dn, off_mos_zap, off_fs, off_fs_dn) = if new_gen {
            (
                OFF_MOS_NEW,
                OFF_MOS_DNODES_NEW,
                OFF_MOS_ZAP_NEW,
                OFF_FS_NEW,
                OFF_FS_DNODES_NEW,
            )
        } else {
            (
                OFF_MOS_OLD,
                OFF_MOS_DNODES_OLD,
                OFF_MOS_ZAP_OLD,
                OFF_FS_OLD,
                OFF_FS_DNODES_OLD,
            )
        };
        let (off_rootdir, off_leaf1, off_subdir, off_fs_master) = if new_gen {
            (
                OFF_ROOTDIR_ZAP_NEW,
                OFF_LEAF1_NEW,
                OFF_SUBDIR_ZAP_NEW,
                OFF_FS_MASTER_NEW,
            )
        } else {
            (
                OFF_ROOTDIR_ZAP_OLD,
                OFF_LEAF1_OLD,
                OFF_SUBDIR_ZAP_OLD,
                OFF_FS_MASTER_OLD,
            )
        };

        // FS master node zap (microzap)
        let fs_master_block = build_microzap(&[
            ("ROOT".to_string(), 2),
            ("SA_ATTRS".to_string(), 5),
            ("VERSION".to_string(), 5),
        ]);
        let fs_master_bp = blkptr(
            off_fs_master,
            1024,
            1024,
            1024,
            2,
            7,
            21,
            0,
            &fs_master_block,
        );
        put(&mut img, off_fs_master, &fs_master_block);

        // Root dir: fatzap (16 KiB). Entries: hello.txt->3, sub->4.
        // The old generation keeps both; the new generation also keeps
        // both (only inner.bin is deleted between txgs).
        let root_entries: Vec<(String, u64)> =
            vec![("hello.txt".to_string(), 3), ("sub".to_string(), 4)];
        let (root_hdr, root_leaf) = build_fatzap_u64(&root_entries, 16384);
        let root_bp = blkptr(off_rootdir, 16384, 16384, 16384, 2, 7, 20, 0, &root_hdr);
        let root_leaf_bp = blkptr(off_leaf1, 16384, 16384, 16384, 2, 7, 20, 0, &root_leaf);
        put(&mut img, off_rootdir, &root_hdr);
        put(&mut img, off_leaf1, &root_leaf);

        // Sub dir zap: old has inner.bin->7; new is empty.
        let sub_entries: Vec<(String, u64)> = if new_gen {
            vec![]
        } else {
            vec![("inner.bin".to_string(), 7)]
        };
        let sub_block = build_microzap(&sub_entries);
        let sub_bp = blkptr(off_subdir, 1024, 1024, 1024, 2, 7, 20, 0, &sub_block);
        put(&mut img, off_subdir, &sub_block);

        // dnodes
        let dn1 = {
            let mut d = dnode_bytes(21, 0);
            d.blkptr0 = fs_master_bp;
            d.datablkszsec = 2;
            d.finish()
        };
        let dn2 = {
            let mut d = dnode_bytes(20, 44);
            d.bonus = sa_bonus(1_500_000_000, 1_700_000_000, 0, 3, 2, 7, 0o040755, 0, 0);
            d.blkptr0 = root_bp;
            d.blkptr1 = Some(root_leaf_bp);
            d.nblkptr = 2;
            d.datablkszsec = 32; // 16 KiB fatzap blocks
            d.finish()
        };
        let dn3 = {
            let mut d = dnode_bytes(19, 44);
            d.bonus = sa_bonus(
                1_600_000_000,
                HELLO_MTIME,
                HELLO_SIZE,
                1,
                2,
                10,
                0o100644,
                1000,
                1000,
            );
            d.blkptr0 = hello_bp;
            d.maxblkid = 0;
            d.finish()
        };
        let dn4 = {
            let mut d = dnode_bytes(20, 44);
            d.bonus = sa_bonus(1_550_000_000, 1_650_000_000, 0, 2, 2, 8, 0o040755, 0, 0);
            d.blkptr0 = sub_bp;
            d.datablkszsec = 2;
            d.finish()
        };
        let dn5 = {
            let mut d = dnode_bytes(45, 0);
            d.blkptr0 = sa_master_bp;
            d.datablkszsec = 2;
            d.finish()
        };
        let dn6 = {
            let mut d = dnode_bytes(46, 0);
            d.blkptr0 = registry_bp;
            d.datablkszsec = 2;
            d.finish()
        };
        let dn7: Vec<u8> = if new_gen {
            vec![0u8; 512] // freed: inner.bin deleted
        } else {
            let mut d = dnode_bytes(19, 44);
            d.bonus = sa_bonus(1_500_000_000, INNER_MTIME, 4096, 1, 4, 9, 0o100600, 0, 0);
            d.blkptr0 = inner_bp;
            d.maxblkid = 0;
            d.finish()
        };
        let dn8 = {
            let mut d = dnode_bytes(47, 0);
            d.blkptr0 = layouts_bp;
            d.blkptr1 = Some(layouts_leaf_bp);
            d.nblkptr = 2;
            d.datablkszsec = 1; // 512 B fatzap blocks
            d.finish()
        };

        let mut fs_dn = vec![0u8; 16384];
        for (i, dn) in [&dn1, &dn2, &dn3, &dn4, &dn5, &dn6, &dn7, &dn8]
            .iter()
            .enumerate()
        {
            fs_dn[i * 512..(i + 1) * 512].copy_from_slice(dn);
        }
        let fs_meta_bp = blkptr(off_fs_dn, 16384, 16384, 16384, 2, 7, 10, 0, &fs_dn);
        put(&mut img, off_fs_dn, &fs_dn);

        let fs_block = build_objset(&fs_meta_bp, 32);
        let fs_rootbp = blkptr(off_fs, 4096, 4096, 4096, 2, 7, 11, 0, &fs_block);
        put(&mut img, off_fs, &fs_block);

        // MOS
        let mos_zap_block = build_microzap(&[("root_dataset".to_string(), 2)]);
        let mos_zap_bp = blkptr(off_mos_zap, 1024, 1024, 1024, 2, 7, 1, 0, &mos_zap_block);
        put(&mut img, off_mos_zap, &mos_zap_block);

        let dn_mz = {
            let mut d = dnode_bytes(1, 0);
            d.blkptr0 = mos_zap_bp;
            d.datablkszsec = 2;
            d.finish()
        };
        let dn_ds = {
            let mut d = dnode_bytes(16, 0);
            let mut bonus = vec![0u8; 256];
            bonus[128..256].copy_from_slice(&fs_rootbp);
            d.bonus = bonus;
            d.finish()
        };
        let mut mos_dn = vec![0u8; 16384];
        mos_dn[0..512].copy_from_slice(&dn_mz);
        mos_dn[512..1024].copy_from_slice(&dn_ds);
        let mos_meta_bp = blkptr(off_mos_dn, 16384, 16384, 16384, 2, 7, 10, 0, &mos_dn);
        put(&mut img, off_mos_dn, &mos_dn);

        let mos_block = build_objset(&mos_meta_bp, 32);
        let mos_rootbp = blkptr(off_mos, 4096, 4096, 4096, 2, 7, 11, 0, &mos_block);
        put(&mut img, off_mos, &mos_block);

        ROOT_BPS.with(|c| {
            if new_gen {
                c.borrow_mut().1 = mos_rootbp.to_vec();
            } else {
                c.borrow_mut().0 = mos_rootbp.to_vec();
            }
        });
    }

    // ---- labels ----
    let ub_old = build_uberblock(TXG_OLD, 1_750_000_000);
    let ub_new = build_uberblock(TXG_NEW, 1_760_000_000);
    let vdev_phys = build_vdev_phys();
    for base in [
        0u64,
        256 * 1024,
        POOL_SIZE - 512 * 1024,
        POOL_SIZE - 256 * 1024,
    ] {
        put(&mut img, base + 16 * 1024, &vdev_phys);
        let ring = base + 128 * 1024;
        put(&mut img, ring, &ub_old);
        put(&mut img, ring + 1024, &ub_new);
    }

    img
}
