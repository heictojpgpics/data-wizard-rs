//! Btrfs superblock (4096 bytes). Offsets are compiled against the kernel
//! UAPI header `include/uapi/linux/btrfs_tree.h` (see
//! `re-evidence/specs/offsets` in the research workspace); this module
//! hard-codes the same table and cites it.
//!
//! Copies: primary at 64 KiB, mirrors at 64 MiB and (when the volume is
//! large enough) 256 GiB. The reader validates each copy (crc32c over
//! bytes 32..4096, stored in the first 4 bytes of `csum`) and selects the
//! copy with the highest generation.

use crate::error::{BtrfsError, BtrfsResult};
use time::OffsetDateTime;

/// `"_BHRfS_M"` as a little-endian u64 (kernel `BTRFS_MAGIC`).
pub const BTRFS_MAGIC: u64 = 0x4D5F_5366_5248_425F;
/// Superblock copy offsets (kernel `BTRFS_SUPER_INFO_OFFSET` et al.).
pub const SB_COPIES: [u64; 3] = [65_536, 64 * 1024 * 1024, 256 * 1024 * 1024 * 1024];
/// crc32c over the first 4 bytes of `csum`; everything after `csum` is
/// covered.
const CSUM_TYPE_CRC32C: u16 = 0;
/// Sanity caps mirroring btrfs-progs mkfs constraints.
const MIN_NODESIZE: u32 = 512;
const MAX_NODESIZE: u32 = 64 * 1024;
const MIN_SECTORSIZE: u32 = 256;
const MAX_SECTORSIZE: u32 = 8 * 1024;
const MAX_NUM_DEVICES: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperBlock {
    pub bytenr: u64,
    pub generation: u64,
    pub root: u64,
    pub chunk_root: u64,
    pub total_bytes: u64,
    pub bytes_used: u64,
    pub num_devices: u64,
    pub sectorsize: u32,
    pub nodesize: u32,
    pub csum_type: u16,
    pub root_level: u8,
    pub chunk_root_level: u8,
    pub incompat_flags: u64,
    pub fsid: [u8; 16],
    pub label: String,
    pub sys_chunk_array: Vec<u8>,
    pub sys_chunk_array_size: u32,
}

impl SuperBlock {
    /// Parse and validate one superblock copy. `buf` is 4096 bytes read
    /// at one of [`SB_COPIES`].
    pub fn parse(buf: &[u8; 4096]) -> BtrfsResult<Self> {
        let magic = u64le(buf, 0x40);
        if magic != BTRFS_MAGIC {
            return Err(BtrfsError::NotBtrfs);
        }
        // csum covers everything after the 32-byte csum field; the value
        // sits in csum[0..4]. Only crc32c (type 0) is accepted here.
        let csum_type = u16le(buf, 0xC4);
        if csum_type != CSUM_TYPE_CRC32C {
            return Err(BtrfsError::Unsupported(format!(
                "csum type {csum_type} (only crc32c is handled)"
            )));
        }
        let expect = u32le(buf, 0);
        let actual = crc32c(&buf[32..4096]);
        if expect != actual {
            return Err(BtrfsError::Corrupt(format!(
                "superblock csum mismatch: stored {expect:#010x}, computed {actual:#010x}"
            )));
        }

        let bytenr = u64le(buf, 0x30);
        if bytenr != SB_COPIES[0] && bytenr != SB_COPIES[1] && bytenr != SB_COPIES[2] {
            // Some images carry 0 here on old kernels; only trust the
            // known copies.
            return Err(BtrfsError::Corrupt(format!(
                "superblock bytenr {bytenr} unexpected"
            )));
        }

        let sectorsize = u32le(buf, 0x90);
        let nodesize = u32le(buf, 0x94);
        if !(MIN_SECTORSIZE..=MAX_SECTORSIZE).contains(&sectorsize) || !sectorsize.is_power_of_two()
        {
            return Err(BtrfsError::Corrupt(format!(
                "sectorsize {sectorsize} invalid"
            )));
        }
        if !(MIN_NODESIZE..=MAX_NODESIZE).contains(&nodesize)
            || !nodesize.is_multiple_of(sectorsize)
        {
            return Err(BtrfsError::Corrupt(format!(
                "nodesize {nodesize} invalid for sectorsize {sectorsize}"
            )));
        }
        let num_devices = u64le(buf, 0x88);
        if num_devices == 0 || num_devices > MAX_NUM_DEVICES {
            return Err(BtrfsError::Corrupt(format!(
                "num_devices {num_devices} invalid"
            )));
        }
        let total_bytes = u64le(buf, 0x70);
        if total_bytes == 0 || total_bytes > (1 << 48) {
            return Err(BtrfsError::Corrupt("total_bytes out of range".into()));
        }

        let mut fsid = [0u8; 16];
        fsid.copy_from_slice(&buf[0x20..0x30]);

        let raw_label = &buf[0x12B..0x12B + 256];
        let label_end = raw_label.iter().position(|&b| b == 0).unwrap_or(256);
        let label = String::from_utf8_lossy(&raw_label[..label_end])
            .trim()
            .to_string();

        let sys_chunk_array_size = u32le(buf, 0xA0);
        if sys_chunk_array_size as usize > 2048 {
            return Err(BtrfsError::Corrupt(format!(
                "sys_chunk_array_size {sys_chunk_array_size} exceeds 2048"
            )));
        }

        Ok(Self {
            bytenr,
            generation: u64le(buf, 0x48),
            root: u64le(buf, 0x50),
            chunk_root: u64le(buf, 0x58),
            total_bytes,
            bytes_used: u64le(buf, 0x78),
            num_devices,
            sectorsize,
            nodesize,
            csum_type,
            root_level: buf[0xC6],
            chunk_root_level: buf[0xC7],
            incompat_flags: u64le(buf, 0xBC),
            fsid,
            label,
            sys_chunk_array: buf[0x32B..0x32B + sys_chunk_array_size as usize].to_vec(),
            sys_chunk_array_size,
        })
    }

    /// Read the best valid copy from `read` (highest generation wins;
    /// ties keep the earlier copy). `None` when no copy parses.
    #[must_use]
    pub fn read_best(read: &dyn dw_disk::ReadAt) -> Option<(Self, u64)> {
        let mut best: Option<(Self, u64)> = None;
        let size = read.size();
        for &off in &SB_COPIES {
            if off + 4096 > size {
                continue;
            }
            let mut buf = vec![0u8; 4096];
            if read.read_at(off, &mut buf).is_err() {
                continue;
            }
            let mut fixed = [0u8; 4096];
            fixed.copy_from_slice(&buf);
            match Self::parse(&fixed) {
                Ok(sb) => {
                    let better = best
                        .as_ref()
                        .map(|(b, _)| sb.generation > b.generation)
                        .unwrap_or(true);
                    if better {
                        best = Some((sb, off));
                    }
                }
                Err(_) => continue,
            }
        }
        best
    }
}

/// crc32c (Castagnoli) as used by Btrfs csum type 0.
/// The `crc` crate's catalog name for it is CRC_32_ISCSI; the table is
/// built once per process. Verified against the reference vector
/// crc32c("123456789") = 0xE3069283 in the tests below.
pub fn crc32c(data: &[u8]) -> u32 {
    use std::sync::OnceLock;
    static TABLE: OnceLock<crc::Crc<u32>> = OnceLock::new();
    TABLE
        .get_or_init(|| crc::Crc::<u32>::new(&crc::CRC_32_ISCSI))
        .checksum(data)
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes([
        b[o],
        b[o + 1],
        b[o + 2],
        b[o + 3],
        b[o + 4],
        b[o + 5],
        b[o + 6],
        b[o + 7],
    ])
}

/// Not used by the parser (times come from inode items) but shared with
/// the walker: convert a btrfs timespec to an OffsetDateTime.
pub fn btrfs_time(sec: u64, nsec: u32) -> Option<OffsetDateTime> {
    if sec == 0 {
        return None;
    }
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(sec) * 1_000_000_000 + i128::from(nsec))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_sb() -> [u8; 4096] {
        let mut b = [0u8; 4096];
        b[0x40..0x48].copy_from_slice(&BTRFS_MAGIC.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&SB_COPIES[0].to_le_bytes()); // bytenr
        b[0x48..0x50].copy_from_slice(&7u64.to_le_bytes()); // generation
        b[0x50..0x58].copy_from_slice(&0x2000u64.to_le_bytes()); // root
        b[0x58..0x60].copy_from_slice(&0x1000u64.to_le_bytes()); // chunk_root
        b[0x70..0x78].copy_from_slice(&(1024 * 1024u64).to_le_bytes()); // total
        b[0x78..0x80].copy_from_slice(&0x8000u64.to_le_bytes()); // used
        b[0x88..0x90].copy_from_slice(&1u64.to_le_bytes()); // num_devices
        b[0x90..0x94].copy_from_slice(&4096u32.to_le_bytes()); // sectorsize
        b[0x94..0x98].copy_from_slice(&4096u32.to_le_bytes()); // nodesize
        b[0xA0..0xA4].copy_from_slice(&0u32.to_le_bytes()); // sys array empty
        b[0xC4..0xC6].copy_from_slice(&0u16.to_le_bytes()); // crc32c
        b[0x12B..0x12B + 9].copy_from_slice(b"TESTBTRFS");
        // csum over 32..4096
        let c = crc32c(&b[32..4096]);
        b[0..4].copy_from_slice(&c.to_le_bytes());
        b
    }

    #[test]
    fn crc32c_reference_vector() {
        // The canonical CRC-32C check value.
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn parses_and_verifies() {
        let sb = SuperBlock::parse(&base_sb()).expect("valid");
        assert_eq!(sb.root, 0x2000);
        assert_eq!(sb.chunk_root, 0x1000);
        assert_eq!(sb.nodesize, 4096);
        assert_eq!(sb.label, "TESTBTRFS");
        assert_eq!(sb.generation, 7);
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut b = base_sb();
        b[0x40] = b'X';
        // recompute csum so the failure is the magic, not the checksum
        let c = crc32c(&b[32..4096]);
        b[0..4].copy_from_slice(&c.to_le_bytes());
        assert!(matches!(SuperBlock::parse(&b), Err(BtrfsError::NotBtrfs)));
    }

    #[test]
    fn rejects_bad_csum() {
        let mut b = base_sb();
        b[0x48] ^= 0xFF; // corrupt generation -> csum changes
        assert!(matches!(SuperBlock::parse(&b), Err(BtrfsError::Corrupt(_))));
    }

    #[test]
    fn rejects_insanity() {
        for (off, val, why) in [
            (0x94, 128u32, "nodesize too small"),
            (0x94, 128 * 1024u32, "nodesize too big"),
            (0x90, 300u32, "sectorsize not pow2"),
            (0x88, 0u32, "num_devices zero"),
        ] {
            let mut b = [0u8; 4096];
            b.copy_from_slice(&base_sb());
            match off {
                0x94 | 0x90 => b[off..off + 4].copy_from_slice(&val.to_le_bytes()),
                0x88 => b[off..off + 8].copy_from_slice(&u64::from(val).to_le_bytes()),
                _ => unreachable!(),
            }
            #[allow(clippy::cast_possible_truncation)]
            let c = crc32c(&b[32..4096]);
            b[0..4].copy_from_slice(&c.to_le_bytes());
            let r = SuperBlock::parse(&b);
            assert!(r.is_err(), "should reject {why}, got {r:?}");
        }
    }

    #[test]
    fn rejects_unknown_csum_type() {
        let mut b = base_sb();
        b[0xC4..0xC6].copy_from_slice(&1u16.to_le_bytes());
        let c = crc32c(&b[32..4096]);
        b[0..4].copy_from_slice(&c.to_le_bytes());
        assert!(matches!(
            SuperBlock::parse(&b),
            Err(BtrfsError::Unsupported(_))
        ));
    }
}
