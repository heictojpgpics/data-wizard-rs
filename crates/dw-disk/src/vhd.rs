//! VHD (Virtual Hard Disk, Microsoft "Virtual Hard Disk Image Format
//! Specification" v1.0) image support — fixed and dynamic disks.
//!
//! Recovery tools receive VHDs regularly (Hyper-V exports, Windows
//! Backup, forensics images); until now the file picker accepted
//! `.vhd` files that every plugin then correctly rejected, because a
//! VHD is a *container*: the raw guest disk is not at file offset 0.
//!
//! Layout (spec, all little-endian):
//!
//! **Footer** (last 512 bytes of the file):
//! ```text
//!   0x00  "conectix" cookie
//!   0x08  features u32
//!   0x0C  file format version u32
//!   0x10  data offset u64   (0xFFFF..FFFF = fixed; else the dynamic
//!                            disk header's offset, normally 512)
//!   0x18  timestamp u32     (seconds since 2000-01-01 UTC)
//!   0x1C  creator application (4)
//!   0x20  creator version u32
//!   0x24  creator host OS u32
//!   0x28  original size u64
//!   0x30  current size u64  (the guest disk's byte size)
//!   0x38  disk geometry (cylinders u16, heads u8, sectors u8)
//!   0x3C  disk type u32     (2 = fixed, 3 = dynamic, 4 = differencing)
//!   0x40  checksum u32      (one's complement of the byte sum with
//!                            the checksum field zeroed)
//!   0x44  unique id (16)
//!   0x54  saved state u8, ...
//! ```
//!
//! **Dynamic disk header** (at `footer.data_offset`, 1024 bytes):
//! ```text
//!   0x00  "cxsparse" cookie
//!   0x08  data offset u64 (unused, 0xFFFF..FFFF)
//!   0x10  table offset u64 (absolute BAT position)
//!   0x18  header version u32
//!   0x1C  max table entries u32
//!   0x20  block size u32    (typically 2 MiB or 16 MiB)
//!   0x24  checksum u32
//!   0x28  parent unique id, parent timestamp, parent name, locators
//! ```
//!
//! **BAT** (`max table entries` × u32 sector numbers; 0xFFFFFFFF =
//! unallocated). Block `i` lives at `BAT[i] * 512` and begins with the
//! block's sector bitmap (`block_size / 512 / 8` bytes, padded to a
//! 512-byte multiple) followed by `block_size` bytes of guest data.
//!
//! Reads of unallocated blocks return zeros (sparse semantics — the
//! NTFS engine then sees a clean MFT zone, exactly like a fresh disk).
//!
//! Differencing disks are refused with a typed error (parent chains
//! are out of scope; the fallback is raw carving, never a wrong
//! parse).

use std::io::{Read, Seek, SeekFrom};

use crate::error::{DiskError, DiskResult};
use crate::source::ReadAt;

const FOOTER_COOKIE: &[u8; 8] = b"conectix";
const DYNAMIC_COOKIE: &[u8; 8] = b"cxsparse";
const DISK_TYPE_FIXED: u32 = 2;
const DISK_TYPE_DYNAMIC: u32 = 3;
const DISK_TYPE_DIFFERENCING: u32 = 4;
const BAT_UNALLOCATED: u32 = 0xFFFF_FFFF;

/// One parsed VHD (header-level view).
#[derive(Debug, Clone)]
pub struct VhdInfo {
    /// Guest disk size in bytes (footer `current size`).
    pub disk_size: u64,
    /// Block size for dynamic disks.
    pub block_size: u32,
    /// BAT sector numbers (empty for fixed disks).
    pub bat: Vec<u32>,
    /// Absolute file offset of the first block's data area start
    /// (BAT area end, dynamic only).
    pub is_dynamic: bool,
}

/// Parse the footer from the last 512 bytes of the file.
fn parse_footer(buf: &[u8]) -> DiskResult<(u64, u64, u32)> {
    if buf.len() < 512 {
        return Err(DiskError::Corrupt("VHD footer too short".into()));
    }
    if &buf[0..8] != FOOTER_COOKIE {
        return Err(DiskError::Corrupt("VHD footer cookie mismatch".into()));
    }
    let rd64 = |o: usize| u64::from_le_bytes(buf[o..o + 8].try_into().unwrap());
    let rd32 = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let data_offset = rd64(0x10);
    let current_size = rd64(0x30);
    let disk_type = rd32(0x3C);
    if current_size == 0 || current_size % 512 != 0 {
        return Err(DiskError::Corrupt(format!(
            "VHD current size {current_size} invalid"
        )));
    }
    Ok((data_offset, current_size, disk_type))
}

/// Verify the footer checksum (one's complement of the zeroed-field
/// byte sum). Recovery-grade tolerance: a mismatch is logged by the
/// caller but does not refuse the image — deleted/resent images with
/// torn footers still parse.
pub fn footer_checksum_ok(buf: &[u8]) -> bool {
    if buf.len() < 512 {
        return false;
    }
    let stored = u32::from_le_bytes(buf[0x40..0x44].try_into().unwrap());
    let mut sum: u32 = 0;
    for (i, &b) in buf.iter().enumerate().take(512) {
        if (0x40..0x44).contains(&i) {
            continue;
        }
        sum = sum.wrapping_add(u32::from(b));
    }
    stored == !sum
}

/// Open a VHD file: sniff the footer, parse the dynamic header + BAT
/// when needed, and return the source plus its info.
pub fn open_vhd(path: &std::path::Path) -> DiskResult<VhdSource> {
    let mut f = std::fs::File::open(path).map_err(DiskError::Io)?;
    let file_size = f.metadata().map_err(DiskError::Io)?.len();
    if file_size < 1024 {
        return Err(DiskError::Corrupt("file too small for a VHD".into()));
    }

    // Footer = last 512 bytes.
    f.seek(SeekFrom::End(-512)).map_err(DiskError::Io)?;
    let mut footer = [0u8; 512];
    f.read_exact(&mut footer).map_err(DiskError::Io)?;
    let (data_offset, disk_size, disk_type) = parse_footer(&footer)?;

    match disk_type {
        DISK_TYPE_FIXED => {
            // Raw data occupies [0, file_size - 512).
            if disk_size > file_size - 512 {
                return Err(DiskError::Corrupt(
                    "fixed VHD claims more data than the file holds".into(),
                ));
            }
            Ok(VhdSource {
                file: std::sync::Arc::new(f),
                info: VhdInfo {
                    disk_size,
                    block_size: 0,
                    bat: Vec::new(),
                    is_dynamic: false,
                },
                data_start: 0,
                _bitmap_bytes: 0,
            })
        }
        DISK_TYPE_DYNAMIC => {
            // Dynamic disk header at data_offset (normally 512).
            f.seek(SeekFrom::Start(data_offset))
                .map_err(DiskError::Io)?;
            let mut header = [0u8; 1024];
            f.read_exact(&mut header).map_err(DiskError::Io)?;
            if &header[0..8] != DYNAMIC_COOKIE {
                return Err(DiskError::Corrupt(
                    "dynamic VHD header cookie mismatch".into(),
                ));
            }
            let table_offset = u64::from_le_bytes(header[0x10..0x18].try_into().unwrap());
            let max_entries = u32::from_le_bytes(header[0x1C..0x20].try_into().unwrap());
            let block_size = u32::from_le_bytes(header[0x20..0x24].try_into().unwrap());
            if block_size == 0 || !block_size.is_multiple_of(512) || max_entries == 0 {
                return Err(DiskError::Corrupt(
                    "dynamic VHD header geometry invalid".into(),
                ));
            }
            // The BAT must cover the whole disk.
            let need = disk_size.div_ceil(u64::from(block_size));
            if u64::from(max_entries) < need {
                return Err(DiskError::Corrupt(
                    "dynamic VHD BAT smaller than the disk requires".into(),
                ));
            }
            // Read the BAT.
            f.seek(SeekFrom::Start(table_offset))
                .map_err(DiskError::Io)?;
            let mut bat = vec![0u8; max_entries as usize * 4];
            f.read_exact(&mut bat).map_err(DiskError::Io)?;
            let bat: Vec<u32> = bat
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let bitmap_bytes = sector_bitmap_bytes(block_size);
            let data_start = table_offset + u64::from(max_entries) * 4;
            Ok(VhdSource {
                file: std::sync::Arc::new(f),
                info: VhdInfo {
                    disk_size,
                    block_size,
                    bat,
                    is_dynamic: true,
                },
                data_start,
                _bitmap_bytes: bitmap_bytes,
            })
        }
        DISK_TYPE_DIFFERENCING => Err(DiskError::UnsupportedUri(
            "differencing VHD (parent chains are not supported; carve raw instead)".into(),
        )),
        other => Err(DiskError::Corrupt(format!("unknown VHD disk type {other}"))),
    }
}

/// Positioned exact read across platforms (Unix `pread`, Windows
/// `seek_read` loop).
fn read_exact_at(file: &std::fs::File, buf: &mut [u8], offset: u64) -> DiskResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        FileExt::read_exact_at(file, buf, offset).map_err(DiskError::Io)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0usize;
        while done < buf.len() {
            let n = file
                .seek_read(&mut buf[done..], offset + done as u64)
                .map_err(DiskError::Io)?;
            if n == 0 {
                return Err(DiskError::ShortRead {
                    offset,
                    got: done as u64,
                    requested: buf.len() as u64,
                    source_size: 0,
                });
            }
            done += n;
        }
        Ok(())
    }
}

fn sector_bitmap_bytes(block_size: u32) -> u32 {
    // block_size / 512 / 8 bytes, padded up to a 512-byte multiple.
    let bits = block_size / 512 / 8;
    bits.div_ceil(512) * 512
}

/// A VHD-backed raw-disk view implementing [`ReadAt`] and
/// [`crate::DiskSource`].
pub struct VhdSource {
    file: std::sync::Arc<std::fs::File>,
    info: VhdInfo,
    /// Dynamic: file offset just past the BAT (block data region
    /// starts here per BAT sector numbers — BAT values are absolute
    /// sectors, so this is only a sanity floor).
    data_start: u64,
    _bitmap_bytes: u32,
}

/// Owned `ReadAt` handle sharing the VHD's file and block map (what
/// `DiskSource::as_read_at` hands to the engines).
pub struct VhdReadAtHandle {
    file: std::sync::Arc<std::fs::File>,
    info: VhdInfo,
    _data_start: u64,
}

impl ReadAt for VhdReadAtHandle {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        vhd_read_at(&self.file, &self.info, offset, buf)
    }
    fn size(&self) -> u64 {
        self.info.disk_size
    }
}

impl VhdSource {
    pub fn info(&self) -> &VhdInfo {
        &self.info
    }
}

#[async_trait::async_trait]
impl crate::DiskSource for VhdSource {
    async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<bytes::Bytes> {
        if len > crate::source::MAX_SINGLE_READ {
            return Err(DiskError::RequestTooLarge {
                len,
                cap: crate::source::MAX_SINGLE_READ,
            });
        }
        let mut buf = vec![0u8; len as usize];
        ReadAt::read_at(self, offset, &mut buf)?;
        Ok(bytes::Bytes::from(buf))
    }

    async fn geometry(&self) -> DiskResult<crate::DiskGeometry> {
        Ok(crate::DiskGeometry {
            total_bytes: self.info.disk_size,
            bytes_per_sector: 512,
            sectors_per_track: 0,
            heads: 0,
            cylinders: 0,
        })
    }

    fn as_read_at(&self) -> std::sync::Arc<dyn ReadAt> {
        std::sync::Arc::new(VhdReadAtHandle {
            file: std::sync::Arc::clone(&self.file),
            info: self.info.clone(),
            _data_start: self.data_start,
        })
    }
}

/// The shared positioned-read engine: maps guest ranges through the
/// block table (dynamic) or reads raw (fixed).
fn vhd_read_at(
    file: &std::fs::File,
    info: &VhdInfo,
    offset: u64,
    buf: &mut [u8],
) -> DiskResult<()> {
    if offset.saturating_add(buf.len() as u64) > info.disk_size {
        return Err(DiskError::OutOfRange {
            offset,
            len: buf.len() as u64,
            source_size: info.disk_size,
        });
    }
    if buf.is_empty() {
        return Ok(());
    }
    if !info.is_dynamic {
        read_exact_at(file, buf, offset)?;
        return Ok(());
    }
    let bs = u64::from(info.block_size);
    let mut done = 0usize;
    while done < buf.len() {
        let guest = offset + done as u64;
        let block = (guest / bs) as usize;
        let within = guest % bs;
        let seg = ((bs - within) as usize).min(buf.len() - done);
        let Some(&sector) = info.bat.get(block) else {
            return Err(DiskError::Corrupt(format!("BAT entry {block} missing")));
        };
        if sector == BAT_UNALLOCATED {
            buf[done..done + seg].fill(0);
        } else {
            let bitmap = u64::from(sector_bitmap_bytes(info.block_size));
            let file_off = u64::from(sector) * 512 + bitmap + within;
            read_exact_at(file, &mut buf[done..done + seg], file_off)?;
        }
        done += seg;
    }
    Ok(())
}

impl ReadAt for VhdSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        vhd_read_at(&self.file, &self.info, offset, buf)
    }

    fn size(&self) -> u64 {
        self.info.disk_size
    }
}

/// Sniff whether a file is a VHD (checks the last-512-byte footer
/// cookie). Cheap: reads only the tail.
pub fn looks_like_vhd(path: &std::path::Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(size) = f.metadata().map(|m| m.len()) else {
        return false;
    };
    if size < 1024 {
        return false;
    }
    if f.seek(SeekFrom::End(-512)).is_err() {
        return false;
    }
    let mut tail = [0u8; 8];
    use std::io::Read as _;
    f.read_exact(&mut tail).is_ok() && &tail == FOOTER_COOKIE
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use std::io::Write;

    fn footer(disk_size: u64, disk_type: u32, data_offset: u64) -> [u8; 512] {
        let mut f = [0u8; 512];
        f[0..8].copy_from_slice(FOOTER_COOKIE);
        f[0x10..0x18].copy_from_slice(&data_offset.to_le_bytes());
        f[0x28..0x30].copy_from_slice(&disk_size.to_le_bytes()); // original
        f[0x30..0x38].copy_from_slice(&disk_size.to_le_bytes()); // current
        f[0x3C..0x40].copy_from_slice(&disk_type.to_le_bytes());
        // checksum: one's complement of the byte sum with the field
        // zeroed.
        let mut sum: u32 = 0;
        for (i, &b) in f.iter().enumerate() {
            if (0x40..0x44).contains(&i) {
                continue;
            }
            sum = sum.wrapping_add(u32::from(b));
        }
        f[0x40..0x44].copy_from_slice(&(!sum).to_le_bytes());
        f
    }

    #[test]
    fn fixed_vhd_roundtrip() {
        // Raw guest data + footer.
        let mut guest = vec![0u8; 4096];
        for (i, b) in guest.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("fixed.vhd");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(&guest).unwrap();
        f.write_all(&footer(4096, DISK_TYPE_FIXED, u64::MAX))
            .unwrap();
        drop(f);

        assert!(looks_like_vhd(&p));
        let src = open_vhd(&p).unwrap();
        assert!(!src.info().is_dynamic);
        assert_eq!(ReadAt::size(&src), 4096);
        let mut out = vec![0u8; 4096];
        src.read_at(0, &mut out).unwrap();
        assert_eq!(out, guest);
        // Partial read at an offset.
        let mut part = [0u8; 16];
        src.read_at(1000, &mut part).unwrap();
        assert_eq!(&part, &guest[1000..1016]);
        // Out of range refused.
        assert!(src.read_at(4090, &mut [0u8; 8]).is_err());
    }

    #[test]
    fn dynamic_vhd_roundtrip_with_sparse_hole() {
        // 2 blocks of 4096 bytes; block 0 allocated, block 1 sparse.
        let bs: u32 = 4096;
        let disk_size: u64 = 8192;
        let bitmap = sector_bitmap_bytes(bs); // 512

        // Layout: [footer copy 512][dyn header 1024][BAT 2*4][block0
        // (bitmap+data)][footer 512]
        let table_offset: u64 = 512 + 1024;
        // Blocks live at sector-aligned positions after the BAT.
        let block0_sector: u32 = ((table_offset + 8).div_ceil(512)) as u32;
        let block0_off = u64::from(block0_sector) * 512;

        let p = dir_file("dyn.vhd");
        let mut f = std::fs::File::create(&p).unwrap();
        // Footer copy at 0 (spec: dynamic disks carry a copy).
        f.write_all(&footer(disk_size, DISK_TYPE_DYNAMIC, 512))
            .unwrap();
        // Dynamic header.
        let mut h = [0u8; 1024];
        h[0..8].copy_from_slice(DYNAMIC_COOKIE);
        h[0x10..0x18].copy_from_slice(&table_offset.to_le_bytes());
        h[0x1C..0x20].copy_from_slice(&2u32.to_le_bytes()); // max entries
        h[0x20..0x24].copy_from_slice(&bs.to_le_bytes());
        f.write_all(&h).unwrap();
        // BAT: block 0 allocated, block 1 empty; then pad to the
        // block's sector-aligned start.
        f.write_all(&block0_sector.to_le_bytes()).unwrap();
        f.write_all(&BAT_UNALLOCATED.to_le_bytes()).unwrap();
        let block0_file_off = u64::from(block0_sector) * 512;
        let bat_end = table_offset + 8;
        let pad = (block0_file_off - bat_end) as usize;
        f.write_all(&vec![0u8; pad]).unwrap();
        // Block 0: bitmap (all-ones = every sector present) + data.
        let mut bitmap_buf = vec![0u8; bitmap as usize];
        bitmap_buf[0] = 0xFF; // first 8 sectors present
        f.write_all(&bitmap_buf).unwrap();
        let data0: Vec<u8> = (0..bs).map(|i| (i % 199) as u8).collect();
        f.write_all(&data0).unwrap();
        // Pad to block-aligned end? Not required; footer last.
        f.write_all(&footer(disk_size, DISK_TYPE_DYNAMIC, 512))
            .unwrap();
        drop(f);

        let src = open_vhd(&p).unwrap();
        assert!(src.info().is_dynamic);
        assert_eq!(ReadAt::size(&src), 8192);

        // Block 0 reads back exactly.
        let mut out0 = vec![0u8; bs as usize];
        src.read_at(0, &mut out0).unwrap();
        assert_eq!(out0, data0);

        // Block 1 is a sparse hole: zeros.
        let mut out1 = vec![0u8; bs as usize];
        src.read_at(bs as u64, &mut out1).unwrap();
        assert!(out1.iter().all(|&b| b == 0));

        // A read spanning both blocks: first half data, second zeros.
        let mut span = vec![0u8; (bs * 2) as usize];
        src.read_at(0, &mut span).unwrap();
        assert_eq!(&span[..bs as usize], &data0[..]);
        assert!(span[bs as usize..].iter().all(|&b| b == 0));
        let _ = block0_off;
        let _ = std::fs::remove_file(&p);
    }

    fn dir_file(name: &str) -> std::path::PathBuf {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(name);
        // Keep the dir alive for the test's duration by leaking it:
        // tests remove the file themselves; the directory is cleaned by
        // the OS temp policy afterwards.
        std::mem::forget(d);
        p
    }

    #[test]
    fn checksum_validation() {
        let f = footer(1024, DISK_TYPE_FIXED, u64::MAX);
        assert!(footer_checksum_ok(&f));
        let mut bad = f;
        bad[0] ^= 0xFF;
        assert!(!footer_checksum_ok(&bad));
    }

    #[test]
    fn differencing_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("diff.vhd");
        let mut f = std::fs::File::create(&p).unwrap();
        // Footer copy at 0, header space, real footer at the end (the
        // layout every real differencing disk carries).
        f.write_all(&footer(1024, DISK_TYPE_DIFFERENCING, 512))
            .unwrap();
        f.write_all(&[0u8; 512]).unwrap();
        f.write_all(&footer(1024, DISK_TYPE_DIFFERENCING, 512))
            .unwrap();
        drop(f);
        match open_vhd(&p) {
            Err(e @ DiskError::UnsupportedUri(_)) => {
                assert!(e.to_string().contains("differencing"));
            }
            Ok(_) => panic!("expected UnsupportedUri, got Ok"),
            Err(other) => panic!("expected UnsupportedUri, got {other}"),
        }
    }

    #[test]
    fn non_vhd_rejected_by_sniff() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("plain.img");
        std::fs::write(&p, vec![0x41u8; 8192]).unwrap();
        assert!(!looks_like_vhd(&p));
    }
}
