//! ReFS boot sector ("FSRS"), validated against a real Windows-formatted
//! ReFS 3.14 volume.
//!
//! Layout (offsets from volume start):
//! ```text
//! 0x003..0x007  "ReFS"   (ASCII)
//! 0x010..0x014  "FSRS"   (ASCII)
//! 0x014..0x016  u16      (per-format value; not verified as a checksum -
//!                        the v3.14 coverage could not be pinned, so no
//!                        checksum claim is made)
//! 0x018..0x020  u64      sector count
//! 0x020..0x024  u32      bytes per sector (must be 512)
//! 0x024..0x028  u32      sectors per cluster (4 KiB or 64 KiB clusters)
//! 0x028..0x02a  u16      version major.minor (3.x)
//! 0x038..0x040  u64      volume serial
//! ```

use crate::error::{RefsError, RefsResult};

pub const SECTOR: u64 = 512;
pub const MAGIC_REFS: &[u8; 4] = b"ReFS";
pub const MAGIC_FSRS: &[u8; 4] = b"FSRS";

#[derive(Debug, Clone)]
pub struct BootSector {
    pub sector_count: u64,
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub version_major: u16,
    pub version_minor: u16,
    pub serial: u64,
}

impl BootSector {
    pub fn parse(vol: &[u8]) -> RefsResult<BootSector> {
        if vol.len() < 512 {
            return Err(RefsError::TooSmall { size: vol.len() as u64 });
        }
        if &vol[0x03..0x07] != MAGIC_REFS || &vol[0x10..0x14] != MAGIC_FSRS {
            return Err(RefsError::NotRefs);
        }
        let sector_count = u64::from_le_bytes(vol[0x18..0x20].try_into().unwrap());
        let bytes_per_sector = u32::from_le_bytes(vol[0x20..0x24].try_into().unwrap());
        let sectors_per_cluster = u32::from_le_bytes(vol[0x24..0x28].try_into().unwrap());
        let version_major = u16::from_le_bytes(vol[0x28..0x2a].try_into().unwrap());
        let version_minor = u16::from_le_bytes(vol[0x2a..0x2c].try_into().unwrap());
        let serial = u64::from_le_bytes(vol[0x38..0x40].try_into().unwrap());

        if bytes_per_sector != 512 {
            return Err(RefsError::BadGeometry {
                reason: format!("bytes per sector {bytes_per_sector} != 512"),
            });
        }
        let cluster = u64::from(sectors_per_cluster) * 512;
        if cluster != 4096 && cluster != 65536 {
            return Err(RefsError::BadGeometry {
                reason: format!("cluster size {cluster} is neither 4 KiB nor 64 KiB"),
            });
        }
        if version_major != 3 {
            return Err(RefsError::BadGeometry {
                reason: format!("version {version_major}.{version_minor} is not 3.x"),
            });
        }
        Ok(BootSector {
            sector_count,
            bytes_per_sector,
            sectors_per_cluster,
            version_major,
            version_minor,
            serial,
        })
    }

    pub fn cluster_size(&self) -> u64 {
        u64::from(self.sectors_per_cluster) * 512
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot_bytes() -> Vec<u8> {
        // Mirrors the real CI-generated sample (ReFS 3.14, 4 KiB clusters).
        let mut v = vec![0u8; 512];
        v[0x03..0x07].copy_from_slice(b"ReFS");
        v[0x10..0x14].copy_from_slice(b"FSRS");
        v[0x14..0x16].copy_from_slice(&0xd5cbu16.to_le_bytes());
        v[0x18..0x20].copy_from_slice(&0x3e0000u64.to_le_bytes());
        v[0x20..0x24].copy_from_slice(&512u32.to_le_bytes());
        v[0x24..0x28].copy_from_slice(&8u32.to_le_bytes());
        v[0x28..0x2a].copy_from_slice(&3u16.to_le_bytes());
        v[0x2a..0x2c].copy_from_slice(&14u16.to_le_bytes());
        v[0x38..0x40].copy_from_slice(&0x2c1877f51877bc80u64.to_le_bytes());
        v
    }

    #[test]
    fn parse_real_layout() {
        let b = BootSector::parse(&boot_bytes()).unwrap();
        assert_eq!(b.sector_count, 0x3e0000);
        assert_eq!(b.cluster_size(), 4096);
        assert_eq!((b.version_major, b.version_minor), (3, 14));
        assert_eq!(b.serial, 0x2c1877f51877bc80);
    }

    #[test]
    fn reject_non_refs() {
        let mut v = boot_bytes();
        v[0x03..0x07].copy_from_slice(b"XXXX");
        assert!(matches!(BootSector::parse(&v), Err(RefsError::NotRefs)));
    }

    #[test]
    fn reject_bad_geometry() {
        let mut v = boot_bytes();
        v[0x24..0x28].copy_from_slice(&3u32.to_le_bytes()); // 1536B clusters
        assert!(matches!(
            BootSector::parse(&v),
            Err(RefsError::BadGeometry { .. })
        ));
    }

    #[test]
    fn reject_non_3x() {
        let mut v = boot_bytes();
        v[0x28..0x2a].copy_from_slice(&4u16.to_le_bytes());
        assert!(matches!(
            BootSector::parse(&v),
            Err(RefsError::BadGeometry { .. })
        ));
    }
}
