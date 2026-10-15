//! NTFS Volume Boot Record (VBR) parser.
//!
//! Layout (public spec: MS-NTFS, "Volume Boot Sector" section):
//!
//! ```text
//! Offset  Size  Field
//!   0     3     Jump instruction (0xEB 0x52 0x90)
//!   3     8     OEM ID ("NTFS    ")
//!  11    25     BIOS Parameter Block (BPB)
//!  36    48     Extended BPB
//!  84   420     Bootstrap code
//! 504     4     Sector signature (0xAA55)
//! ```
//!
//! BPB (offset 11, 25 bytes):
//!
//! ```text
//!  11   2  bytes_per_sector  (512, 1024, 2048, 4096)
//!  13   1  sectors_per_cluster (1, 2, 4, 8, 16, 32, 64, 128)
//!  14   2  reserved_sectors (always 0 for NTFS)
//!  16   1  number_of_fats (always 0)
//!  17   2  root_entries (always 0)
//!  19   2  total_sectors_16 (always 0)
//!  21   1  media_descriptor (0xF8 for fixed disk)
//!  22   2  sectors_per_fat (always 0)
//!  24   2  sectors_per_track
//!  26   2  number_of_heads
//!  28   4  hidden_sectors
//!  32   2  (unused, always 0)
//! ```
//!
//! Extended BPB (offset 36, 48 bytes):
//!
//! ```text
//!  36   8  total_sectors_64
//!  44   8  mft_logical_cluster_number
//!  52   8  mft_mirror_logical_cluster_number
//!  60   1  clusters_per_mft_record (signed: if < 0, |val| is the
//!                                    log2 of bytes per MFT record;
//!                                    else it's the cluster count)
//!  61   1  clusters_per_index_record (same encoding)
//!  62   8  volume_serial_number
//!  70   4  checksum
//! ```

use bytemuck::{Pod, Zeroable};
use thiserror::Error;

/// NTFS VBR as it appears on disk. We use `bytemuck::Pod` for zero-copy
/// reinterpretation of the boot sector buffer.
///
/// Layout follows MS-NTFS Open Specifications "Volume Boot Sector".
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VbrRaw {
    pub jump: [u8; 3],
    pub oem_id: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub number_of_fats: u8,
    pub root_entries: u16,
    pub total_sectors_16: u16,
    pub media_descriptor: u8,
    pub sectors_per_fat: u16,
    pub sectors_per_track: u16,
    pub number_of_heads: u16,
    pub hidden_sectors: u32,
    pub total_sectors_32: u32, // always 0 in NTFS, but the field exists
    pub total_sectors_64: u64,
    pub mft_logical_cluster_number: u64,
    pub mft_mirror_logical_cluster_number: u64,
    pub clusters_per_mft_record_raw: i8,
    pub clusters_per_index_record_raw: i8,
    pub volume_serial_number: u64,
    pub checksum: u32,
    // Bootstrap code + signature follow, not parsed here.
}

/// Parsed VBR.
#[derive(Debug, Clone)]
pub struct Vbr {
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub bytes_per_cluster: u32,
    pub total_sectors: u64,
    pub total_bytes: u64,
    pub mft_logical_cluster_number: u64,
    pub mft_mirror_logical_cluster_number: u64,
    pub mft_record_size_bytes: u32,
    pub index_record_size_bytes: u32,
    pub volume_serial_number: u64,
}

const NTFS_OEM_ID: [u8; 8] = *b"NTFS    ";
const BOOT_SIGNATURE: u16 = 0xAA55;

#[derive(Debug, Error)]
pub enum VbrError {
    #[error("not an NTFS volume: OEM ID mismatch (got {0:?}, expected {1:?})")]
    NotNtfs([u8; 8], [u8; 8]),
    #[error("invalid bytes_per_sector: {0} (must be 512/1024/2048/4096)")]
    InvalidBytesPerSector(u16),
    #[error("invalid sectors_per_cluster: {0} (must be 1/2/4/8/16/32/64/128)")]
    InvalidSectorsPerCluster(u8),
    #[error("invalid boot signature: 0x{0:04X} (expected 0xAA55)")]
    InvalidBootSignature(u16),
    #[error("invalid MFT record size encoding: {0}")]
    InvalidMftRecordSize(i8),
    #[error("invalid index record size encoding: {0}")]
    InvalidIndexRecordSize(i8),
    #[error("buffer too short: got {got}, need 512")]
    BufferTooShort { got: usize },
}

impl Vbr {
    /// Parse a 512-byte boot sector.
    ///
    /// Returns `Err(VbrError::NotNtfs)` if the OEM ID doesn't match —
    /// callers should treat this as "this isn't an NTFS volume" and try
    /// the next FS plugin.
    pub fn parse(buf: &[u8; 512]) -> Result<Self, VbrError> {
        if buf.len() < 512 {
            return Err(VbrError::BufferTooShort { got: buf.len() });
        }

        // SAFETY: `VbrRaw` is `repr(C, packed)` and `Pod`, and `buf` is
        // 512 bytes which is much larger than `size_of::<VbrRaw>()`. The
        // reference is valid for the lifetime of `buf`.
        let raw: &VbrRaw = bytemuck::from_bytes(&buf[..std::mem::size_of::<VbrRaw>()]);

        if raw.oem_id != NTFS_OEM_ID {
            return Err(VbrError::NotNtfs(raw.oem_id, NTFS_OEM_ID));
        }

        let bytes_per_sector = match raw.bytes_per_sector {
            512 | 1024 | 2048 | 4096 => u32::from(raw.bytes_per_sector),
            other => return Err(VbrError::InvalidBytesPerSector(other)),
        };

        let sectors_per_cluster = match raw.sectors_per_cluster {
            1 | 2 | 4 | 8 | 16 | 32 | 64 | 128 => u32::from(raw.sectors_per_cluster),
            other => return Err(VbrError::InvalidSectorsPerCluster(other)),
        };

        // Boot signature (last 2 bytes of the 512-byte sector).
        let sig = u16::from_le_bytes([buf[510], buf[511]]);
        if sig != BOOT_SIGNATURE {
            return Err(VbrError::InvalidBootSignature(sig));
        }

        let bytes_per_cluster = bytes_per_sector * sectors_per_cluster;

        let mft_record_size_bytes =
            decode_record_size(raw.clusters_per_mft_record_raw, bytes_per_cluster)
                .map_err(VbrError::InvalidMftRecordSize)?;

        let index_record_size_bytes =
            decode_record_size(raw.clusters_per_index_record_raw, bytes_per_cluster)
                .map_err(VbrError::InvalidIndexRecordSize)?;

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            bytes_per_cluster,
            total_sectors: raw.total_sectors_64,
            total_bytes: raw.total_sectors_64 * u64::from(bytes_per_sector),
            mft_logical_cluster_number: raw.mft_logical_cluster_number,
            mft_mirror_logical_cluster_number: raw.mft_mirror_logical_cluster_number,
            mft_record_size_bytes,
            index_record_size_bytes,
            volume_serial_number: raw.volume_serial_number,
        })
    }

    /// Byte offset of the MFT from the start of the volume.
    #[must_use]
    pub fn mft_offset_bytes(&self) -> u64 {
        self.mft_logical_cluster_number * u64::from(self.bytes_per_cluster)
    }

    /// Byte offset of the MFT mirror from the start of the volume.
    #[must_use]
    pub fn mft_mirror_offset_bytes(&self) -> u64 {
        self.mft_mirror_logical_cluster_number * u64::from(self.bytes_per_cluster)
    }
}

/// Decode the MFT / index record size encoding.
///
/// Per MS-NTFS: the byte is signed. If it's positive, it's the number of
/// clusters per record. If it's negative, the record size is
/// `1 << -value` bytes (so -10 means 1024 bytes, the typical MFT record
/// size).
fn decode_record_size(raw: i8, bytes_per_cluster: u32) -> Result<u32, i8> {
    if raw >= 0 {
        // Positive: clusters per record. 0 is invalid.
        if raw == 0 {
            return Err(raw);
        }
        Ok(u32::from(raw as u8) * bytes_per_cluster)
    } else {
        // Negative: log2 of the record size in bytes.
        let log2 = -i32::from(raw);
        if !(1..=31).contains(&log2) {
            return Err(raw);
        }
        Ok(1u32 << log2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_vbr_buf() -> [u8; 512] {
        let mut buf = [0u8; 512];
        // Jump instruction
        buf[0..3].copy_from_slice(&[0xEB, 0x52, 0x90]);
        // OEM ID
        buf[3..11].copy_from_slice(b"NTFS    ");
        // bytes_per_sector = 512
        buf[11..13].copy_from_slice(&512u16.to_le_bytes());
        // sectors_per_cluster = 8
        buf[13] = 8;
        // media_descriptor = 0xF8 (fixed disk)
        buf[21] = 0xF8;
        // sectors_per_track = 63
        buf[24..26].copy_from_slice(&63u16.to_le_bytes());
        // number_of_heads = 255
        buf[26..28].copy_from_slice(&255u16.to_le_bytes());
        // total_sectors_64 (offset 36) = 1_000_000 sectors
        buf[36..44].copy_from_slice(&1_000_000u64.to_le_bytes());
        // mft_logical_cluster_number (offset 44) = 4
        buf[44..52].copy_from_slice(&4u64.to_le_bytes());
        // mft_mirror_logical_cluster_number (offset 52) = 100
        buf[52..60].copy_from_slice(&100u64.to_le_bytes());
        // clusters_per_mft_record_raw (offset 60) = -10 (=> 1024 bytes)
        buf[60] = (-10i8) as u8;
        // clusters_per_index_record_raw (offset 61) = -12 (=> 4096 bytes)
        buf[61] = (-12i8) as u8;
        // volume_serial_number (offset 62) = arbitrary
        buf[62..70].copy_from_slice(&0xDEADBEEFCAFEBABEu64.to_le_bytes());
        // Boot signature at 510..512
        buf[510..512].copy_from_slice(&BOOT_SIGNATURE.to_le_bytes());
        buf
    }

    #[test]
    fn parses_valid_vbr() {
        let buf = make_valid_vbr_buf();
        let vbr = Vbr::parse(&buf).expect("parse");
        assert_eq!(vbr.bytes_per_sector, 512);
        assert_eq!(vbr.sectors_per_cluster, 8);
        assert_eq!(vbr.bytes_per_cluster, 4096);
        assert_eq!(vbr.total_sectors, 1_000_000);
        assert_eq!(vbr.total_bytes, 1_000_000 * 512);
        assert_eq!(vbr.mft_logical_cluster_number, 4);
        assert_eq!(vbr.mft_mirror_logical_cluster_number, 100);
        assert_eq!(vbr.mft_record_size_bytes, 1024);
        assert_eq!(vbr.index_record_size_bytes, 4096);
        assert_eq!(vbr.mft_offset_bytes(), 4 * 4096);
        assert_eq!(vbr.mft_mirror_offset_bytes(), 100 * 4096);
    }

    #[test]
    fn rejects_non_ntfs_oem() {
        let mut buf = make_valid_vbr_buf();
        buf[3..11].copy_from_slice(b"FAT32   ");
        let err = Vbr::parse(&buf).expect_err("should reject");
        assert!(matches!(err, VbrError::NotNtfs(_, _)));
    }

    #[test]
    fn rejects_invalid_bytes_per_sector() {
        let mut buf = make_valid_vbr_buf();
        buf[11..13].copy_from_slice(&1024u16.to_le_bytes()); // 1024 is valid
        let _ = Vbr::parse(&buf).expect("1024 is valid");
        buf[11..13].copy_from_slice(&2048u16.to_le_bytes()); // 2048 is valid
        let _ = Vbr::parse(&buf).expect("2048 is valid");
        buf[11..13].copy_from_slice(&333u16.to_le_bytes()); // invalid
        let err = Vbr::parse(&buf).expect_err("should reject 333");
        assert!(matches!(err, VbrError::InvalidBytesPerSector(333)));
    }

    #[test]
    fn rejects_invalid_sectors_per_cluster() {
        let mut buf = make_valid_vbr_buf();
        buf[13] = 3; // not a power of 2 in the allowed set
        let err = Vbr::parse(&buf).expect_err("should reject");
        assert!(matches!(err, VbrError::InvalidSectorsPerCluster(3)));
    }

    #[test]
    fn rejects_invalid_boot_signature() {
        let mut buf = make_valid_vbr_buf();
        buf[510..512].copy_from_slice(&0x0000u16.to_le_bytes());
        let err = Vbr::parse(&buf).expect_err("should reject");
        assert!(matches!(err, VbrError::InvalidBootSignature(0)));
    }

    #[test]
    fn rejects_invalid_mft_record_size() {
        let mut buf = make_valid_vbr_buf();
        buf[60] = 0; // 0 is invalid
        let err = Vbr::parse(&buf).expect_err("should reject");
        assert!(matches!(err, VbrError::InvalidMftRecordSize(0)));
    }

    #[test]
    fn positive_clusters_per_mft_record_works() {
        let mut buf = make_valid_vbr_buf();
        buf[60] = 2; // 2 clusters per MFT record
        let vbr = Vbr::parse(&buf).expect("parse");
        // 2 clusters * 4096 bytes/cluster = 8192 bytes
        assert_eq!(vbr.mft_record_size_bytes, 8192);
    }

    #[test]
    fn negative_log2_encoding_works() {
        let mut buf = make_valid_vbr_buf();
        buf[60] = (-10i8) as u8; // 1 << 10 = 1024
        let vbr = Vbr::parse(&buf).expect("parse");
        assert_eq!(vbr.mft_record_size_bytes, 1024);

        let mut buf = make_valid_vbr_buf();
        buf[60] = (-11i8) as u8; // 1 << 11 = 2048
        let vbr = Vbr::parse(&buf).expect("parse");
        assert_eq!(vbr.mft_record_size_bytes, 2048);

        let mut buf = make_valid_vbr_buf();
        buf[60] = (-12i8) as u8; // 1 << 12 = 4096
        let vbr = Vbr::parse(&buf).expect("parse");
        assert_eq!(vbr.mft_record_size_bytes, 4096);
    }
}
