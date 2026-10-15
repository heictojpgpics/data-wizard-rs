//! exFAT Boot Sector parser.
//!
//! Layout (public spec: Microsoft "Extended FAT File System Specification"
//! v1.00, 2008; also Linux kernel `fs/exfat/`):
//!
//! The boot region spans 12 sectors (6144 bytes). Sectors 0, 1, 2, 3, 4, 5
//! are the Main Boot Region (with 3 backups at sectors 8, 9, 10, 11, 12, 13).
//! Sector 0 is the Main Boot Sector; sector 1 is the Extended Boot Sectors
//! (sectors 1-8); sector 9 is the OEM parameters; sector 10 is reserved;
//! sector 11 is the boot checksum.
//!
//! ## Main Boot Sector (sector 0, 512 bytes)
//!
//! ```text
//! Offset  Size  Field
//!   0     3     JumpBoot (0xEB 0x76 0x90)
//!   3     8     FileSystemName ("EXFAT   ")
//!  11     53    MustBeZero (all zeros — distinguishes from FAT)
//!  64     8     PartitionOffset (LBA of this partition start)
//!  72     8     VolumeLength (in sectors)
//!  80     4     FatOffset (in sectors, from partition start)
//!  84     4     FatLength (in sectors)
//!  88     4     ClusterHeapOffset (in sectors, from partition start)
//!  92     4     ClusterCount
//!  96     4     RootDirFirstCluster
//! 100     4     VolumeSerialNumber
//! 104     2     FileSystemRevision (0x0100 = v1.00)
//! 106     2     VolumeFlags (bit 0 = ActiveFat, bit 1 = VolumeDirty,
//!                         bit 2 = MediaFailure, bit 3 = ClearToZero)
//! 108     1     BytesPerSectorShift (log2 of bytes per sector: 9=512, 12=4096)
//! 109     1     SectorsPerClusterShift (log2 of sectors per cluster)
//! 110     1     NumberOfFats (1 or 2)
//! 111     1     DriveSelect (0x80 for fixed disk)
//! 112     1     PercentInUse (0-100 or 0xFF = unknown)
//! 113     7     Reserved
//! 120   390     BootCode
//! 510     2     BootSignature (0xAA55)
//! ```
//!
//! ## Extended Boot Sectors (sectors 1-8)
//!
//! Each is 512 bytes, same structure as the Main Boot Sector but with a
//! different jump instruction and the first 3 bytes are `0xEB 0x?? 0x90`.
//! The last 2 bytes are still `0xAA55`.
//!
//! ## Boot Checksum (sector 11)
//!
//! A 4-byte CRC32 of the first 11 sectors (sectors 0-10), repeated 128
//! times to fill the 512-byte sector.

use bytemuck::{Pod, Zeroable};

use crate::error::{ExfatError, ExfatResult};

/// exFAT OEM magic.
const EXFAT_OEM_ID: [u8; 8] = *b"EXFAT   ";
const BOOT_SIGNATURE: u16 = 0xAA55;
const FILE_SYSTEM_REVISION_1_00: u16 = 0x0100;

/// Main Boot Sector raw layout (first 120 bytes of sector 0).
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ExfatBootSectorRaw {
    pub jump_boot: [u8; 3],
    pub file_system_name: [u8; 8],
    pub must_be_zero: [u8; 53],
    pub partition_offset: u64,
    pub volume_length_sectors: u64,
    pub fat_offset: u32,
    pub fat_length: u32,
    pub cluster_heap_offset: u32,
    pub cluster_count: u32,
    pub root_dir_first_cluster: u32,
    pub volume_serial_number: u32,
    pub file_system_revision: u16,
    pub volume_flags: u16,
    pub bytes_per_sector_shift: u8,
    pub sectors_per_cluster_shift: u8,
    pub number_of_fats: u8,
    pub drive_select: u8,
    pub percent_in_use: u8,
    pub reserved: [u8; 7],
    // BootCode follows (390 bytes), then boot signature at 510.
}

/// Parsed exFAT boot sector.
#[derive(Debug, Clone)]
pub struct ExfatBoot {
    pub partition_offset: u64,
    pub volume_length_sectors: u64,
    pub fat_offset: u32,
    pub fat_length: u32,
    pub cluster_heap_offset: u32,
    pub cluster_count: u32,
    pub root_dir_first_cluster: u32,
    pub volume_serial_number: u32,
    pub file_system_revision: u16,
    pub volume_flags: u16,
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub bytes_per_cluster: u32,
    pub number_of_fats: u8,
    pub percent_in_use: u8,
}

/// Volume flags (public spec).
pub mod volume_flags {
    pub const ACTIVE_FAT: u16 = 0x0001;
    pub const VOLUME_DIRTY: u16 = 0x0002;
    pub const MEDIA_FAILURE: u16 = 0x0004;
    pub const CLEAR_TO_ZERO: u16 = 0x0008;
}

impl ExfatBoot {
    /// Parse a 512-byte boot sector.
    ///
    /// Returns `Err(ExfatError::NotExfat)` if the OEM ID doesn't match
    /// — callers should treat this as "this isn't an exFAT volume" and
    /// try the next FS plugin.
    pub fn parse(buf: &[u8; 512]) -> ExfatResult<Self> {
        let raw: &ExfatBootSectorRaw =
            bytemuck::from_bytes(&buf[..std::mem::size_of::<ExfatBootSectorRaw>()]);

        if raw.file_system_name != EXFAT_OEM_ID {
            return Err(ExfatError::NotExfat);
        }

        // The "MustBeZero" field (bytes 11-63) must be all zeros.
        // This distinguishes exFAT from FAT12/16/32 (which have a BPB there).
        if raw.must_be_zero.iter().any(|&b| b != 0) {
            return Err(ExfatError::Corrupt(
                "MustBeZero field is not zero — not a valid exFAT boot sector".into(),
            ));
        }

        // Copy packed fields to locals to avoid unaligned references.
        let partition_offset = raw.partition_offset;
        let volume_length_sectors = raw.volume_length_sectors;
        let fat_offset = raw.fat_offset;
        let fat_length = raw.fat_length;
        let cluster_heap_offset = raw.cluster_heap_offset;
        let cluster_count = raw.cluster_count;
        let root_dir_first_cluster = raw.root_dir_first_cluster;
        let volume_serial_number = raw.volume_serial_number;
        let file_system_revision = raw.file_system_revision;
        let volume_flags = raw.volume_flags;
        let bytes_per_sector_shift = raw.bytes_per_sector_shift;
        let sectors_per_cluster_shift = raw.sectors_per_cluster_shift;
        let number_of_fats = raw.number_of_fats;
        let percent_in_use = raw.percent_in_use;

        // BytesPerSectorShift: valid range is 9 (512 bytes) to 12 (4096 bytes).
        if !(9..=12).contains(&bytes_per_sector_shift) {
            return Err(ExfatError::Corrupt(format!(
                "invalid BytesPerSectorShift: {bytes_per_sector_shift} (must be 9-12)"
            )));
        }
        let bytes_per_sector = 1u32 << bytes_per_sector_shift;

        // SectorsPerClusterShift: valid range is 0-25.
        if sectors_per_cluster_shift > 25 {
            return Err(ExfatError::Corrupt(format!(
                "invalid SectorsPerClusterShift: {sectors_per_cluster_shift} (must be 0-25)"
            )));
        }
        let sectors_per_cluster = 1u32 << sectors_per_cluster_shift;
        let bytes_per_cluster = bytes_per_sector * sectors_per_cluster;

        // NumberOfFats: must be 1 or 2.
        if number_of_fats != 1 && number_of_fats != 2 {
            return Err(ExfatError::Corrupt(format!(
                "invalid NumberOfFats: {number_of_fats} (must be 1 or 2)"
            )));
        }

        // FileSystemRevision: must be 0x0100 (v1.00).
        if file_system_revision != FILE_SYSTEM_REVISION_1_00 {
            return Err(ExfatError::Corrupt(format!(
                "unsupported FileSystemRevision: 0x{file_system_revision:04X} (only 0x0100 supported)"
            )));
        }

        // Boot signature at offset 510.
        let boot_sig = u16::from_le_bytes([buf[510], buf[511]]);
        if boot_sig != BOOT_SIGNATURE {
            return Err(ExfatError::Corrupt(format!(
                "invalid boot signature: 0x{boot_sig:04X} (expected 0x{BOOT_SIGNATURE:04X})"
            )));
        }

        // Sanity checks on offsets.
        if fat_offset == 0 {
            return Err(ExfatError::Corrupt("FatOffset is 0".into()));
        }
        if cluster_heap_offset == 0 {
            return Err(ExfatError::Corrupt("ClusterHeapOffset is 0".into()));
        }
        if cluster_count == 0 {
            return Err(ExfatError::Corrupt("ClusterCount is 0".into()));
        }
        if root_dir_first_cluster < 2 {
            return Err(ExfatError::Corrupt(format!(
                "RootDirFirstCluster is {root_dir_first_cluster} (must be >= 2)"
            )));
        }

        Ok(Self {
            partition_offset,
            volume_length_sectors,
            fat_offset,
            fat_length,
            cluster_heap_offset,
            cluster_count,
            root_dir_first_cluster,
            volume_serial_number,
            file_system_revision,
            volume_flags,
            bytes_per_sector,
            sectors_per_cluster,
            bytes_per_cluster,
            number_of_fats,
            percent_in_use,
        })
    }

    /// Byte offset of the FAT from the start of the volume.
    #[must_use]
    pub fn fat_offset_bytes(&self) -> u64 {
        u64::from(self.fat_offset) * u64::from(self.bytes_per_sector)
    }

    /// Byte offset of the cluster heap (data region) from the start of the volume.
    #[must_use]
    pub fn cluster_heap_offset_bytes(&self) -> u64 {
        u64::from(self.cluster_heap_offset) * u64::from(self.bytes_per_sector)
    }

    /// Convert an exFAT cluster number to a byte offset from the start of the volume.
    ///
    /// exFAT clusters are numbered starting from 2 (clusters 0 and 1 are reserved).
    /// Cluster N is at: `cluster_heap_offset + (N - 2) * bytes_per_cluster`
    #[must_use]
    pub fn cluster_to_byte(&self, cluster: u32) -> u64 {
        self.cluster_heap_offset_bytes()
            + u64::from(cluster.saturating_sub(2)) * u64::from(self.bytes_per_cluster)
    }

    /// Total volume size in bytes.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.volume_length_sectors * u64::from(self.bytes_per_sector)
    }

    /// Is the volume marked dirty (was not cleanly unmounted)?
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        (self.volume_flags & volume_flags::VOLUME_DIRTY) != 0
    }

    /// Which FAT is active (0 or 1)? Only relevant when NumberOfFats == 2.
    #[must_use]
    pub fn active_fat_index(&self) -> u8 {
        if (self.volume_flags & volume_flags::ACTIVE_FAT) != 0 {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_exfat_boot() -> [u8; 512] {
        let mut buf = [0u8; 512];
        // JumpBoot
        buf[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
        // FileSystemName
        buf[3..11].copy_from_slice(b"EXFAT   ");
        // MustBeZero (bytes 11-63) — already zero
        // PartitionOffset (offset 64)
        buf[64..72].copy_from_slice(&0u64.to_le_bytes());
        // VolumeLength (offset 72) = 2_000_000 sectors (~1 GB)
        buf[72..80].copy_from_slice(&2_000_000u64.to_le_bytes());
        // FatOffset (offset 80) = sector 2048
        buf[80..84].copy_from_slice(&2048u32.to_le_bytes());
        // FatLength (offset 84) = 1024 sectors
        buf[84..88].copy_from_slice(&1024u32.to_le_bytes());
        // ClusterHeapOffset (offset 88) = sector 4096
        buf[88..92].copy_from_slice(&4096u32.to_le_bytes());
        // ClusterCount (offset 92) = 100_000 clusters
        buf[92..96].copy_from_slice(&100_000u32.to_le_bytes());
        // RootDirFirstCluster (offset 96) = cluster 2
        buf[96..100].copy_from_slice(&2u32.to_le_bytes());
        // VolumeSerialNumber (offset 100)
        buf[100..104].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
        // FileSystemRevision (offset 104) = 0x0100
        buf[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        // VolumeFlags (offset 106) = 0
        buf[106..108].copy_from_slice(&0u16.to_le_bytes());
        // BytesPerSectorShift (offset 108) = 9 (512 bytes)
        buf[108] = 9;
        // SectorsPerClusterShift (offset 109) = 3 (8 sectors per cluster = 4096 bytes)
        buf[109] = 3;
        // NumberOfFats (offset 110) = 1
        buf[110] = 1;
        // DriveSelect (offset 111) = 0x80
        buf[111] = 0x80;
        // PercentInUse (offset 112) = 0xFF (unknown)
        buf[112] = 0xFF;
        // Reserved (113-119) — already zero
        // Boot signature at 510
        buf[510..512].copy_from_slice(&BOOT_SIGNATURE.to_le_bytes());
        buf
    }

    #[test]
    fn parses_valid_exfat_boot() {
        let buf = make_valid_exfat_boot();
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert_eq!(boot.volume_length_sectors, 2_000_000);
        assert_eq!(boot.fat_offset, 2048);
        assert_eq!(boot.fat_length, 1024);
        assert_eq!(boot.cluster_heap_offset, 4096);
        assert_eq!(boot.cluster_count, 100_000);
        assert_eq!(boot.root_dir_first_cluster, 2);
        assert_eq!(boot.volume_serial_number, 0xDEADBEEF);
        assert_eq!(boot.file_system_revision, 0x0100);
        assert_eq!(boot.bytes_per_sector, 512);
        assert_eq!(boot.sectors_per_cluster, 8);
        assert_eq!(boot.bytes_per_cluster, 4096);
        assert_eq!(boot.number_of_fats, 1);
        assert_eq!(boot.percent_in_use, 0xFF);
    }

    #[test]
    fn rejects_non_exfat_oem() {
        let mut buf = make_valid_exfat_boot();
        buf[3..11].copy_from_slice(b"FAT32   ");
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::NotExfat));
    }

    #[test]
    fn rejects_nonzero_must_be_zero() {
        let mut buf = make_valid_exfat_boot();
        // Corrupt a byte in the MustBeZero region (bytes 11-63)
        buf[20] = 0x01;
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn rejects_invalid_bytes_per_sector_shift() {
        let mut buf = make_valid_exfat_boot();
        buf[108] = 8; // 8 is too small (256 bytes — not valid for exFAT)
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));

        buf[108] = 13; // 13 is too large (8192 bytes — not valid for exFAT)
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn rejects_invalid_sectors_per_cluster_shift() {
        let mut buf = make_valid_exfat_boot();
        buf[109] = 26; // too large
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn rejects_invalid_number_of_fats() {
        let mut buf = make_valid_exfat_boot();
        buf[110] = 3; // must be 1 or 2
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn rejects_invalid_file_system_revision() {
        let mut buf = make_valid_exfat_boot();
        buf[104..106].copy_from_slice(&0x0200u16.to_le_bytes()); // v2.00
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn rejects_invalid_boot_signature() {
        let mut buf = make_valid_exfat_boot();
        buf[510..512].copy_from_slice(&0x0000u16.to_le_bytes());
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn rejects_zero_fat_offset() {
        let mut buf = make_valid_exfat_boot();
        buf[80..84].copy_from_slice(&0u32.to_le_bytes());
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn rejects_invalid_root_dir_cluster() {
        let mut buf = make_valid_exfat_boot();
        buf[96..100].copy_from_slice(&1u32.to_le_bytes()); // must be >= 2
        let err = ExfatBoot::parse(&buf).expect_err("should reject");
        assert!(matches!(err, ExfatError::Corrupt(_)));
    }

    #[test]
    fn cluster_to_byte_works() {
        let buf = make_valid_exfat_boot();
        let boot = ExfatBoot::parse(&buf).expect("parse");
        // Cluster 2 = first data cluster = cluster_heap_offset_bytes
        assert_eq!(boot.cluster_to_byte(2), boot.cluster_heap_offset_bytes());
        // Cluster 3 = cluster_heap_offset_bytes + bytes_per_cluster
        assert_eq!(
            boot.cluster_to_byte(3),
            boot.cluster_heap_offset_bytes() + u64::from(boot.bytes_per_cluster)
        );
        // Cluster 10 = cluster_heap_offset_bytes + 8 * bytes_per_cluster
        assert_eq!(
            boot.cluster_to_byte(10),
            boot.cluster_heap_offset_bytes() + 8 * u64::from(boot.bytes_per_cluster)
        );
    }

    #[test]
    fn total_bytes_works() {
        let buf = make_valid_exfat_boot();
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert_eq!(boot.total_bytes(), 2_000_000 * 512);
    }

    #[test]
    fn fat_offset_bytes_works() {
        let buf = make_valid_exfat_boot();
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert_eq!(boot.fat_offset_bytes(), 2048 * 512);
    }

    #[test]
    fn is_dirty_detects_volume_dirty_flag() {
        let mut buf = make_valid_exfat_boot();
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert!(!boot.is_dirty());

        buf[106..108].copy_from_slice(&volume_flags::VOLUME_DIRTY.to_le_bytes());
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert!(boot.is_dirty());
    }

    #[test]
    fn active_fat_index_works() {
        let mut buf = make_valid_exfat_boot();
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert_eq!(boot.active_fat_index(), 0);

        buf[106..108].copy_from_slice(&volume_flags::ACTIVE_FAT.to_le_bytes());
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert_eq!(boot.active_fat_index(), 1);
    }

    #[test]
    fn handles_4096_byte_sectors() {
        let mut buf = make_valid_exfat_boot();
        // Change to 4096-byte sectors (shift = 12)
        buf[108] = 12;
        // Change to 1 sector per cluster (shift = 0)
        buf[109] = 0;
        let boot = ExfatBoot::parse(&buf).expect("parse");
        assert_eq!(boot.bytes_per_sector, 4096);
        assert_eq!(boot.sectors_per_cluster, 1);
        assert_eq!(boot.bytes_per_cluster, 4096);
    }
}
