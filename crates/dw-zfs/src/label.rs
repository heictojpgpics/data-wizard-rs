//! vdev labels and uberblock rings (`vdev_impl.h`, `uberblock_impl.h`).
//!
//! Label geometry (each label 256 KiB; 4 per device):
//!
//! ```text
//! L0 @ 0            pad1 8K | be 8K | vdev_phys 112K | ub ring 128K
//! L1 @ 256K
//! L2 @ psize-512K
//! L3 @ psize-256K
//! ```
//!
//! `vdev_phys`: nvlist at +0 (112K-40 bytes) with the trailing
//! `zio_eck_t { u64 pad; zio_cksum_t }` checksummed by fletcher4.
//! The uberblock ring starts at +128K inside the label; slot size is
//! `1 << MIN(MAX(ashift, 10), 13)`; the ring holds 128K/slot_size slots.
//! An uberblock is valid iff `ub_magic == 0x00bab10c` (LE, or BE for a
//! whole stream written by a big-endian host - `uberblock.c`).

use crate::checksum;
use crate::dmu::read_exact;
use crate::error::{ZfsError, ZfsResult};
use crate::nvlist::NvList;
use std::sync::atomic::{AtomicBool, Ordering};

pub const VDEV_LABEL_SIZE: u64 = 256 * 1024;
pub const VDEV_PHYS_REGION: u64 = 112 * 1024;
pub const UBERBLOCK_RING: u64 = 128 * 1024;
/// Offset of `vl_vdev_phys` inside a label.
pub const LABEL_PHYS_OFF: u64 = 16 * 1024;
/// Offset of `vl_uberblock` inside a label.
pub const LABEL_UB_OFF: u64 = 128 * 1024;

pub const UBERBLOCK_MAGIC: u64 = 0x00ba_b10c;
pub const SPA_VERSION_FEATURES: u64 = 5000;
/// Largest ub ring slot the format allows (`MAX_UBERBLOCK_SHIFT`).
pub const MAX_UB_SHIFT: u32 = 13;
pub const MIN_UB_SHIFT: u32 = 10;

/// Structured uberblock (216 bytes; fields at the offsets from
/// `struct uberblock`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uberblock {
    pub magic: u64,
    pub version: u64,
    pub txg: u64,
    pub guid_sum: u64,
    pub timestamp: u64,
    /// Byte offset of `ub_rootbp` inside the label (this ub's slot).
    pub rootbp_off: u64,
    /// Which label this ub was read from.
    pub label: u8,
    /// Big-endian stream (magic matched after byteswap).
    pub big_endian: bool,
    pub software_version: u64,
    pub checkpoint_txg: u64,
}

impl Uberblock {
    /// Parse the uberblock at `slot_off` (absolute). `label` indexes the
    /// source label for diagnostics.
    pub fn parse(read: &dyn dw_disk::ReadAt, slot_off: u64, label: u8) -> Option<Uberblock> {
        let raw = read_exact(read, slot_off, 216).ok()?;
        let le = |o: usize| u64::from_le_bytes(raw[o..o + 8].try_into().unwrap());
        let be = |o: usize| u64::from_be_bytes(raw[o..o + 8].try_into().unwrap());
        let big_endian = if le(0) == UBERBLOCK_MAGIC {
            false
        } else if be(0) == UBERBLOCK_MAGIC {
            true
        } else {
            return None;
        };
        let g = |o: usize| {
            if big_endian { be(o) } else { le(o) }
        };
        let ub = Uberblock {
            magic: UBERBLOCK_MAGIC,
            version: g(8),
            txg: g(16),
            guid_sum: g(24),
            timestamp: g(32),
            rootbp_off: slot_off + 40,
            label,
            big_endian,
            software_version: g(168),
            checkpoint_txg: g(200),
        };
        // Sanity: txg 0 is never a synced txg; version must be a known
        // SPA version (<= 91 legacy or 5000 feature flags).
        if ub.txg == 0 {
            return None;
        }
        if ub.version > 91 && ub.version != SPA_VERSION_FEATURES {
            return None;
        }
        Some(ub)
    }
}

/// Decoded pool config from a label nvlist, with the scope decision.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub name: String,
    pub version: u64,
    pub pool_guid: u64,
    pub ashift: u32,
    /// Number of *top-level* vdevs declared by the pool.
    pub top_level_vdevs: usize,
    /// The single top-level vdev is a plain leaf ("disk" or "file").
    pub single_leaf: bool,
}

impl PoolConfig {
    pub fn from_nvlist(nvl: &NvList) -> ZfsResult<PoolConfig> {
        let name = nvl
            .get_str("name")
            .ok_or_else(|| ZfsError::Corrupt("pool config: no name".into()))?
            .to_string();
        let version = nvl.get_u64("version").unwrap_or(SPA_VERSION_FEATURES);
        let pool_guid = nvl.get_u64("pool_guid").unwrap_or(0);
        let tree = nvl
            .get_nvlist("vdev_tree")
            .ok_or_else(|| ZfsError::Corrupt("pool config: no vdev_tree".into()))?;
        let ashift = tree.get_u64("ashift").unwrap_or(9) as u32;
        // Top-level vdevs: the vdev_tree's own `children` array (the tree
        // itself has type "root").
        let children = tree.get_nvlist_array("children").unwrap_or(&[]);
        let top = children.len();
        let single_leaf =
            top == 1 && matches!(children[0].get_str("type"), Some("disk") | Some("file"));
        Ok(PoolConfig {
            name,
            version,
            pool_guid,
            ashift,
            top_level_vdevs: top,
            single_leaf,
        })
    }
}

/// One label's read result.
pub struct LabelInfo {
    pub index: u8,
    pub config: PoolConfig,
    pub nvlist_valid: bool,
}

pub struct LabelSet {
    pub labels: Vec<LabelInfo>,
    pub best: Uberblock,
    /// Second-newest distinct txg (deep pass material).
    pub previous: Option<Uberblock>,
}

/// Read all 4 labels, verify nvlist checksums, and select the best
/// (highest txg, then newest timestamp) plus runner-up uberblock.
pub fn load_labels(
    read: &dyn dw_disk::ReadAt,
    src_size: u64,
    cancel: &AtomicBool,
) -> ZfsResult<LabelSet> {
    if src_size < 4 * VDEV_LABEL_SIZE {
        return Err(ZfsError::SourceTooSmall {
            size: src_size,
            min: 4 * VDEV_LABEL_SIZE,
        });
    }
    let offsets = [
        0u64,
        VDEV_LABEL_SIZE,
        src_size - 2 * VDEV_LABEL_SIZE,
        src_size - VDEV_LABEL_SIZE,
    ];

    let mut labels = Vec::new();
    let mut ubs: Vec<Uberblock> = Vec::new();
    let mut any_checksum_ok = false;

    for (idx, base) in offsets.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ZfsError::Corrupt("cancelled".into()));
        }
        let i = idx as u8;
        // Read the whole vdev_phys region (nvlist + eck).
        let region = read_exact(read, base + LABEL_PHYS_OFF, VDEV_PHYS_REGION as usize).ok();
        let region = match region {
            Some(r) => r,
            None => continue,
        };
        // zio_eck_t { u64 zec_pad; zio_cksum_t zec_cksum }: the cksum is
        // the last 32 bytes of the region (the pad sits before it).
        let expected = checksum::ZioCksum::from_slice(&region[VDEV_PHYS_REGION as usize - 32..]);
        let native = checksum::label_fletcher4(&region);
        let swapped = checksum::fletcher4(&region[..VDEV_PHYS_REGION as usize - 40], true);
        let nvlist_valid = native == expected || swapped == expected;
        if !nvlist_valid {
            continue;
        }
        any_checksum_ok = true;
        // The nvlist body is the region minus the eck.
        let nvl = match NvList::parse(&region[..VDEV_PHYS_REGION as usize - 40]) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let config = match PoolConfig::from_nvlist(&nvl) {
            Ok(c) => c,
            Err(_) => continue,
        };
        labels.push(LabelInfo {
            index: i,
            config,
            nvlist_valid: true,
        });

        // Uberblock ring: try every legal slot size; the first that
        // yields any valid ub in this ring wins (slots at finer
        // granularity than the writer's are a superset of the real ones,
        // so a single pass at 1K covers all shifts >= 10... except it
        // must not miss ubs that only exist at coarser shifts: scanning
        // at 1K finds every real slot because every real slot start is
        // 1K-aligned. Scan once at 1K.)
        let ring_base = base + LABEL_UB_OFF;
        let slots = UBERBLOCK_RING / 1024;
        for s in 0..slots {
            if cancel.load(Ordering::Relaxed) {
                return Err(ZfsError::Corrupt("cancelled".into()));
            }
            let slot_off = ring_base + s * 1024;
            if let Some(ub) = Uberblock::parse(read, slot_off, i) {
                ubs.push(ub);
            }
        }
    }

    if labels.is_empty() || !any_checksum_ok {
        return Err(ZfsError::NoValidLabel);
    }
    if ubs.is_empty() {
        return Err(ZfsError::NoUberblock);
    }

    // Best by (txg, timestamp); runner-up by distinct txg.
    ubs.sort_by(|a, b| b.txg.cmp(&a.txg).then(b.timestamp.cmp(&a.timestamp)));
    let best = ubs[0];
    let previous = ubs.iter().find(|u| u.txg < best.txg).copied();

    Ok(LabelSet {
        labels,
        best,
        previous,
    })
}
