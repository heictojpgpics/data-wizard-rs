//! FAT12/16/32 BIOS Parameter Block (BPB) parser.
//!
//! Layout (public spec: Microsoft Extensible Firmware Initiative FAT32
//! File System Specification, version 1.03, December 6, 2000):
//!
//! ```text
//! Offset  Size  Field                        Notes
//!   0     3     jmpBoot                      0xEB 0x?? 0x90 or 0xE9 ...
//!   3     8     OEMName                      "MSDOS5.0" or "mkfs.fat"
//!  11    25     BPB (shared by FAT12/16/32)
//!  36    26     Extended BPB (FAT12/16)      OR
//!  36    28     Extended BPB (FAT32)
//! ```
//!
//! Shared BPB (offset 11, 25 bytes):
//!
//! ```text
//!  11   2  BytsPerSec      (512/1024/2048/4096)
//!  13   1  SecPerClus      (1/2/4/8/16/32/64/128)
//!  14   2  RsvdSecCnt      (typically 1 for FAT12/16, 32 for FAT32)
//!  16   1  NumFATs         (typically 2)
//!  17   2  RootEntCnt      (FAT12/16: 512 typical; FAT32: 0)
//!  19   2  TotSec16        (0 if > 65535, use TotSec32)
//!  21   1  Media           (0xF8 fixed, 0xF0 removable)
//!  22   2  FATSz16         (FAT12/16; 0 for FAT32, use FATSz32)
//!  24   2  SecPerTrk
//!  26   2  NumHeads
//!  28   4  HiddSec
//!  32   4  TotSec32
//! ```
//!
//! FAT12/16 Extended BPB (offset 36, 26 bytes):
//!
//! ```text
//!  36   1  DrvNum          (0x00 floppy A:, 0x80 fixed C:)
//!  37   1  Reserved1       (0)
//!  38   1  BootSig         (0x29 if following 3 fields present)
//!  39   4  VolID           (volume serial)
//!  43  11  VolLab          (volume label, padded with spaces)
//!  54   8  FilSysType      ("FAT12   ", "FAT16   ")
//! ```
//!
//! FAT32 Extended BPB (offset 36, 28 bytes... actually 26):
//!
//! ```text
//!  36   4  FATSz32
//!  40   2  ExtFlags        (active FAT + mirroring)
//!  42   2  FSVer           (0:0)
//!  44   4  RootClus        (typically 2)
//!  48   2  FSInfo          (typically 1)
//!  50   2  BkBootSec       (typically 6)
//!  52  12  Reserved
//!  64   1  DrvNum
//!  65   1  Reserved1
//!  66   1  BootSig
//!  67   4  VolID
//!  71  11  VolLab
//!  82   8  FilSysType      ("FAT32   ")
//! ```

use bytemuck::{Pod, Zeroable};

use crate::error::{FatError, FatResult};

/// FAT variant.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FatKind {
    Fat12,
    Fat16,
    Fat32,
}

/// Shared BPB (offset 11, 25 bytes) — common to FAT12/16/32.
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BpbShared {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sector_count: u16,
    pub num_fats: u8,
    pub root_entry_count: u16,
    pub total_sectors_16: u16,
    pub media: u8,
    pub fat_size_16: u16,
    pub sectors_per_track: u16,
    pub num_heads: u16,
    pub hidden_sectors: u32,
    pub total_sectors_32: u32,
}

/// FAT32-specific extended BPB fields (offset 36, 28 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BpbExtFat32 {
    pub fat_size_32: u32,
    pub ext_flags: u16,
    pub fs_version: u16,
    pub root_cluster: u32,
    pub fs_info: u16,
    pub bk_boot_sec: u16,
    pub reserved: [u8; 12],
    pub drv_num: u8,
    pub reserved1: u8,
    pub boot_sig: u8,
    pub vol_id: u32,
    pub vol_lab: [u8; 11],
    pub fil_sys_type: [u8; 8],
}

/// Parsed FAT BPB.
#[derive(Debug, Clone)]
pub struct FatBpb {
    pub kind: FatKind,
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub bytes_per_cluster: u32,
    pub reserved_sector_count: u32,
    pub num_fats: u32,
    pub fat_size_sectors: u32,
    pub root_entry_count: u32,
    pub root_dir_sectors: u32,
    pub total_sectors: u64,
    pub media: u8,
    pub volume_id: Option<u32>,
    pub volume_label: Option<String>,
    /// FAT32 only: first cluster of the root directory.
    pub root_cluster: Option<u32>,
    /// FAT32 only: sector number of the FSINFO structure.
    pub fs_info_sector: Option<u16>,
    /// First data sector (after reserved + FATs + root dir region).
    pub first_data_sector: u32,
    /// Total count of data clusters (determines FAT12 vs FAT16 vs FAT32).
    pub count_of_clusters: u32,
}

impl FatBpb {
    /// Parse a 512-byte boot sector.
    pub fn parse(buf: &[u8; 512]) -> FatResult<Self> {
        // Shared BPB at offset 11.
        let shared: &BpbShared =
            bytemuck::from_bytes(&buf[11..11 + std::mem::size_of::<BpbShared>()]);

        let bytes_per_sector = match shared.bytes_per_sector {
            512 | 1024 | 2048 | 4096 => u32::from(shared.bytes_per_sector),
            other => {
                return Err(FatError::Corrupt(format!(
                    "invalid bytes_per_sector: {other}"
                )));
            }
        };

        let sectors_per_cluster = match shared.sectors_per_cluster {
            1 | 2 | 4 | 8 | 16 | 32 | 64 | 128 => u32::from(shared.sectors_per_cluster),
            other => {
                return Err(FatError::Corrupt(format!(
                    "invalid sectors_per_cluster: {other}"
                )));
            }
        };

        if shared.num_fats == 0 {
            return Err(FatError::Corrupt("num_fats == 0".into()));
        }

        let bytes_per_cluster = bytes_per_sector * sectors_per_cluster;
        let reserved_sector_count = u32::from(shared.reserved_sector_count);
        let num_fats = u32::from(shared.num_fats);
        let root_entry_count = u32::from(shared.root_entry_count);

        // Determine total_sectors (16-bit field preferred, fall back to 32-bit).
        let total_sectors = if shared.total_sectors_16 != 0 {
            u64::from(shared.total_sectors_16)
        } else {
            u64::from(shared.total_sectors_32)
        };

        // Determine FAT size (16-bit field preferred, fall back to 32-bit
        // for FAT32).
        let fat_size_sectors = if shared.fat_size_16 != 0 {
            u32::from(shared.fat_size_16)
        } else {
            // Read FATSz32 from the FAT32 extended BPB at offset 36.
            let ext: &BpbExtFat32 =
                bytemuck::from_bytes(&buf[36..36 + std::mem::size_of::<BpbExtFat32>()]);
            ext.fat_size_32
        };

        // Compute root directory sector count (FAT12/16 only).
        // Per spec: RootDirSectors = ceil(RootEntCnt * 32 / BytsPerSec)
        let root_dir_sectors = if root_entry_count == 0 {
            0 // FAT32 — root dir is a cluster chain, not a fixed region
        } else {
            (root_entry_count * 32).div_ceil(bytes_per_sector)
        };

        // First data sector = RsvdSecCnt + (NumFATs * FATSz) + RootDirSectors
        let first_data_sector =
            reserved_sector_count + (num_fats * fat_size_sectors) + root_dir_sectors;

        // Count of data clusters = (TotSec - FirstDataSector) / SecPerClus
        let data_sectors = total_sectors.saturating_sub(u64::from(first_data_sector));
        let count_of_clusters =
            u32::try_from(data_sectors / u64::from(sectors_per_cluster)).unwrap_or(u32::MAX);

        // Determine FAT type per the spec's decision tree.
        let kind = if count_of_clusters < 4085 {
            FatKind::Fat12
        } else if count_of_clusters < 65525 {
            FatKind::Fat16
        } else {
            FatKind::Fat32
        };

        // Parse FAT32-specific fields.
        let (root_cluster, fs_info_sector, volume_id, volume_label) = if kind == FatKind::Fat32 {
            let ext: &BpbExtFat32 =
                bytemuck::from_bytes(&buf[36..36 + std::mem::size_of::<BpbExtFat32>()]);
            let vol_id = if ext.boot_sig == 0x29 {
                Some(ext.vol_id)
            } else {
                None
            };
            let vol_lab = if ext.boot_sig == 0x29 {
                Some(String::from_utf8_lossy(&ext.vol_lab).trim_end().to_string())
            } else {
                None
            };
            (Some(ext.root_cluster), Some(ext.fs_info), vol_id, vol_lab)
        } else {
            // FAT12/16: extended BPB at offset 36 has DrvNum(1) + Reserved1(1)
            // + BootSig(1) + VolID(4) + VolLab(11) + FilSysType(8).
            // We just check BootSig and read VolID/VolLab if present.
            let boot_sig = buf.get(38).copied().unwrap_or(0);
            let (vol_id, vol_lab) = if boot_sig == 0x29 {
                let vol_id = u32::from_le_bytes([buf[39], buf[40], buf[41], buf[42]]);
                let vol_lab = String::from_utf8_lossy(&buf[43..54]).trim_end().to_string();
                (Some(vol_id), Some(vol_lab))
            } else {
                (None, None)
            };
            (None, None, vol_id, vol_lab)
        };

        Ok(Self {
            kind,
            bytes_per_sector,
            sectors_per_cluster,
            bytes_per_cluster,
            reserved_sector_count,
            num_fats,
            fat_size_sectors,
            root_entry_count,
            root_dir_sectors,
            total_sectors,
            media: shared.media,
            volume_id,
            volume_label,
            root_cluster,
            fs_info_sector,
            first_data_sector,
            count_of_clusters,
        })
    }

    /// Byte offset of the first FAT from the start of the volume.
    #[must_use]
    pub fn fat_offset_bytes(&self) -> u64 {
        u64::from(self.reserved_sector_count) * u64::from(self.bytes_per_sector)
    }

    /// Byte offset of the root directory region (FAT12/16) from the start
    /// of the volume. Returns `None` for FAT32 (root dir is a cluster chain).
    #[must_use]
    pub fn root_dir_offset_bytes(&self) -> Option<u64> {
        if self.kind == FatKind::Fat32 {
            None
        } else {
            let sectors_before_root =
                self.reserved_sector_count + (self.num_fats * self.fat_size_sectors);
            Some(u64::from(sectors_before_root) * u64::from(self.bytes_per_sector))
        }
    }

    /// Byte offset of the first data cluster (cluster #2 per the spec).
    #[must_use]
    pub fn first_data_offset_bytes(&self) -> u64 {
        u64::from(self.first_data_sector) * u64::from(self.bytes_per_sector)
    }

    /// Convert a cluster number to a byte offset from the start of the volume.
    /// Cluster numbers are 2-based (clusters 0 and 1 are reserved).
    #[must_use]
    pub fn cluster_to_byte(&self, cluster: u32) -> u64 {
        // Per spec: byte offset = ((cluster - 2) * sectors_per_cluster + first_data_sector) * bytes_per_sector
        u64::from(cluster.saturating_sub(2)) * u64::from(self.bytes_per_cluster)
            + self.first_data_offset_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fat16_buf() -> [u8; 512] {
        let mut buf = [0u8; 512];
        // Jump
        buf[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
        // OEM
        buf[3..11].copy_from_slice(b"MSDOS5.0");
        // BPB
        buf[11..13].copy_from_slice(&512u16.to_le_bytes()); // bytes_per_sector
        buf[13] = 4; // sectors_per_cluster
        buf[14..16].copy_from_slice(&1u16.to_le_bytes()); // reserved_sector_count
        buf[16] = 2; // num_fats
        buf[17..19].copy_from_slice(&512u16.to_le_bytes()); // root_entry_count
        buf[19..21].copy_from_slice(&0u16.to_le_bytes()); // total_sectors_16 = 0 (use 32)
        buf[21] = 0xF8; // media
        buf[22..24].copy_from_slice(&256u16.to_le_bytes()); // fat_size_16
        buf[24..26].copy_from_slice(&63u16.to_le_bytes()); // sectors_per_track
        buf[26..28].copy_from_slice(&255u16.to_le_bytes()); // num_heads
        buf[28..32].copy_from_slice(&0u32.to_le_bytes()); // hidden_sectors
        // total_sectors_32 = 65536 (so count_of_clusters = ~16000 => FAT16)
        buf[32..36].copy_from_slice(&65536u32.to_le_bytes());
        // Extended BPB (FAT12/16)
        buf[36] = 0x80; // drv_num
        buf[37] = 0; // reserved
        buf[38] = 0x29; // boot_sig
        buf[39..43].copy_from_slice(&0xDEADBEEFu32.to_le_bytes()); // vol_id
        buf[43..54].copy_from_slice(b"TESTVOLUME "); // vol_lab (11 bytes)
        buf[54..62].copy_from_slice(b"FAT16   "); // fil_sys_type
        // Boot signature
        buf[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());
        buf
    }

    fn make_fat32_buf() -> [u8; 512] {
        let mut buf = [0u8; 512];
        buf[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
        buf[3..11].copy_from_slice(b"mkfs.fat");
        buf[11..13].copy_from_slice(&512u16.to_le_bytes()); // bytes_per_sector
        buf[13] = 8; // sectors_per_cluster
        buf[14..16].copy_from_slice(&32u16.to_le_bytes()); // reserved_sector_count
        buf[16] = 2; // num_fats
        buf[17..19].copy_from_slice(&0u16.to_le_bytes()); // root_entry_count = 0 (FAT32)
        buf[19..21].copy_from_slice(&0u16.to_le_bytes()); // total_sectors_16 = 0
        buf[21] = 0xF8; // media
        buf[22..24].copy_from_slice(&0u16.to_le_bytes()); // fat_size_16 = 0 (use 32)
        buf[24..26].copy_from_slice(&63u16.to_le_bytes());
        buf[26..28].copy_from_slice(&255u16.to_le_bytes());
        buf[28..32].copy_from_slice(&0u32.to_le_bytes());
        // total_sectors_32 = 1_000_000 => ~125000 clusters => FAT32
        buf[32..36].copy_from_slice(&1_000_000u32.to_le_bytes());
        // FAT32 extended BPB at offset 36
        buf[36..40].copy_from_slice(&2000u32.to_le_bytes()); // fat_size_32
        buf[40..42].copy_from_slice(&0u16.to_le_bytes()); // ext_flags
        buf[42..44].copy_from_slice(&0u16.to_le_bytes()); // fs_version
        buf[44..48].copy_from_slice(&2u32.to_le_bytes()); // root_cluster
        buf[48..50].copy_from_slice(&1u16.to_le_bytes()); // fs_info
        buf[50..52].copy_from_slice(&6u16.to_le_bytes()); // bk_boot_sec
        // reserved 12 bytes (52..64) = 0
        buf[64] = 0x80; // drv_num
        buf[65] = 0; // reserved1
        buf[66] = 0x29; // boot_sig
        buf[67..71].copy_from_slice(&0xCAFEBABEu32.to_le_bytes()); // vol_id
        buf[71..82].copy_from_slice(b"TESTVOLUME "); // vol_lab
        buf[82..90].copy_from_slice(b"FAT32   "); // fil_sys_type
        buf[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());
        buf
    }

    #[test]
    fn parses_fat16() {
        let buf = make_fat16_buf();
        let bpb = FatBpb::parse(&buf).expect("parse");
        assert_eq!(bpb.kind, FatKind::Fat16);
        assert_eq!(bpb.bytes_per_sector, 512);
        assert_eq!(bpb.sectors_per_cluster, 4);
        assert_eq!(bpb.bytes_per_cluster, 2048);
        assert_eq!(bpb.reserved_sector_count, 1);
        assert_eq!(bpb.num_fats, 2);
        assert_eq!(bpb.fat_size_sectors, 256);
        assert_eq!(bpb.root_entry_count, 512);
        assert_eq!(bpb.total_sectors, 65536);
        assert_eq!(bpb.media, 0xF8);
        assert_eq!(bpb.volume_id, Some(0xDEADBEEF));
        assert_eq!(bpb.volume_label.as_deref(), Some("TESTVOLUME"));
        assert!(bpb.root_cluster.is_none());
        // root_dir_sectors = (512 * 32 + 511) / 512 = 32
        assert_eq!(bpb.root_dir_sectors, 32);
        // first_data_sector = 1 + 2*256 + 32 = 545
        assert_eq!(bpb.first_data_sector, 545);
        // count_of_clusters = (65536 - 545) / 4 = 16247
        assert_eq!(bpb.count_of_clusters, 16247);
    }

    #[test]
    fn parses_fat32() {
        let buf = make_fat32_buf();
        let bpb = FatBpb::parse(&buf).expect("parse");
        assert_eq!(bpb.kind, FatKind::Fat32);
        assert_eq!(bpb.bytes_per_sector, 512);
        assert_eq!(bpb.sectors_per_cluster, 8);
        assert_eq!(bpb.bytes_per_cluster, 4096);
        assert_eq!(bpb.reserved_sector_count, 32);
        assert_eq!(bpb.num_fats, 2);
        assert_eq!(bpb.fat_size_sectors, 2000);
        assert_eq!(bpb.root_entry_count, 0);
        assert_eq!(bpb.total_sectors, 1_000_000);
        assert_eq!(bpb.root_cluster, Some(2));
        assert_eq!(bpb.fs_info_sector, Some(1));
        assert_eq!(bpb.volume_id, Some(0xCAFEBABE));
        assert_eq!(bpb.volume_label.as_deref(), Some("TESTVOLUME"));
        // FAT32: root_dir_sectors = 0 (root is a cluster chain)
        assert_eq!(bpb.root_dir_sectors, 0);
        // first_data_sector = 32 + 2*2000 + 0 = 4032
        assert_eq!(bpb.first_data_sector, 4032);
        // count_of_clusters = (1000000 - 4032) / 8 = 124496
        assert_eq!(bpb.count_of_clusters, 124496);
    }

    #[test]
    fn fat_kind_thresholds() {
        // Per spec: < 4085 clusters => FAT12
        //           < 65525 clusters => FAT16
        //           else              => FAT32
        let mut buf = make_fat16_buf();
        // Make it small enough to be FAT12: 4000 clusters with 4 sec/clus
        // total_sectors = first_data_sector + 4000*4 = 545 + 16000 = 16545
        buf[32..36].copy_from_slice(&16545u32.to_le_bytes());
        let bpb = FatBpb::parse(&buf).expect("parse");
        assert_eq!(bpb.kind, FatKind::Fat12);

        // 60000 clusters => FAT16
        let mut buf = make_fat16_buf();
        // total_sectors = 545 + 60000*4 = 240545
        buf[32..36].copy_from_slice(&240545u32.to_le_bytes());
        let bpb = FatBpb::parse(&buf).expect("parse");
        assert_eq!(bpb.kind, FatKind::Fat16);
    }

    #[test]
    fn cluster_to_byte_fat16() {
        let buf = make_fat16_buf();
        let bpb = FatBpb::parse(&buf).expect("parse");
        // first_data_offset = 545 * 512 = 279040
        // cluster 2 = first_data_offset + 0 = 279040
        assert_eq!(bpb.cluster_to_byte(2), 279040);
        // cluster 3 = first_data_offset + 2048 = 281088
        assert_eq!(bpb.cluster_to_byte(3), 281088);
    }

    #[test]
    fn rejects_invalid_bytes_per_sector() {
        let mut buf = make_fat16_buf();
        buf[11..13].copy_from_slice(&333u16.to_le_bytes());
        let err = FatBpb::parse(&buf).expect_err("should reject");
        assert!(matches!(err, FatError::Corrupt(_)));
    }

    #[test]
    fn rejects_zero_num_fats() {
        let mut buf = make_fat16_buf();
        buf[16] = 0;
        let err = FatBpb::parse(&buf).expect_err("should reject");
        assert!(matches!(err, FatError::Corrupt(_)));
    }
}
