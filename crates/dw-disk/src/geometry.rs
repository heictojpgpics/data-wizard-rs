//! Disk geometry and partition-table types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct SectorSize(pub u32);

impl Default for SectorSize {
    fn default() -> Self {
        Self(512)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskGeometry {
    pub total_bytes: u64,
    pub bytes_per_sector: u32,
    pub sectors_per_track: u32,
    pub heads: u32,
    pub cylinders: u32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum PartitionType {
    Empty,
    Mbr,
    Gpt,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum PartitionFsHint {
    Unknown,
    Ntfs,
    Fat32,
    Fat16,
    Exfat,
    Ext4,
    Ext3,
    Ext2,
    HfsPlus,
    Btrfs,
    Refs,
    Zfs,
    Swap,
    Other(u8),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    pub index: u32,
    pub start_byte: u64,
    pub size_bytes: u64,
    pub fs_hint: PartitionFsHint,
    pub label: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionTable {
    pub kind: PartitionType,
    pub partitions: Vec<Partition>,
}
