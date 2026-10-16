//! System Attributes (SA): the post-2008 ZPL metadata encoding. Registry
//! and layout zaps live in the filesystem objset; the file metadata lives
//! in the dnode bonus buffer.
//!
//! Chain (extracted from `zfs_znode.c` / `sa.c` / `sa_impl.h`):
//! - master node (object 1): "SA_ATTRS" -> sa_master_obj,
//!   "ROOT" -> root dir object, "VERSION" -> ZPL version.
//! - sa_master_obj zap: "REGISTRY" -> sa_reg_attr_obj (zap: attr name ->
//!   u64 ATTR_ENCODE{num: bits 0..16, bswap: 16..24, length: 24..40}),
//!   "LAYOUTS" -> sa_layout_attr_obj (zap: decimal layout number ->
//!   u16 array of attribute numbers, in layout order).
//! - bonus: `sa_hdr_phys { u32 magic 0x2F505A; u16 layout_info
//!   (bits 0..10 layout, bits 10..16 hdrsize/8); u16 sa_lengths[]... }`
//!   followed by attribute data in layout order (variable-length attrs
//!   consume sa_lengths entries in order).
//!
//! Legacy (pre-SA) pools store `znode_phys_t` in the bonus: atime@0(16),
//! mtime@16, ctime@32, crtime@48, gen@64, mode@72, size@80, parent@88,
//! links@96, uid@128, gid@136 (`zfs_sa.h`).

use crate::dmu::{Dmu, ObjectTable, Objset};
use crate::error::ZfsResult;
use crate::zap::{self, ZapValue};
use std::collections::HashMap;

pub const SA_MAGIC: u32 = 0x2F_505A;
pub const MASTER_NODE_OBJ: u64 = 1;

/// Decoded file metadata, SA or legacy.
#[derive(Debug, Clone, Default)]
pub struct ZfsAttrs {
    pub size: Option<u64>,
    pub atime: Option<u64>,
    pub mtime: Option<u64>,
    pub ctime: Option<u64>,
    pub crtime: Option<u64>,
    pub links: Option<u64>,
    pub parent: Option<u64>,
    pub generation: Option<u64>,
    pub mode: Option<u64>,
    pub uid: Option<u64>,
    pub gid: Option<u64>,
}

/// The SA registry + layouts for one filesystem objset.
pub struct SaState {
    /// attr number -> (name, registered length; 0 = variable).
    attrs: HashMap<u16, (String, u16)>,
    /// layout number -> attribute numbers in order.
    layouts: HashMap<u16, Vec<u16>>,
}

impl SaState {
    /// Load the registry from the objset's master node.
    pub fn load(dmu: &mut Dmu, table: &mut ObjectTable, os: &Objset) -> ZfsResult<SaState> {
        let master = zap::read_zap(dmu, table, os, MASTER_NODE_OBJ)?;
        let sa_master_obj = zap::zap_u64(&master, "SA_ATTRS").unwrap_or(0);
        let mut attrs = HashMap::new();
        let mut layouts = HashMap::new();
        if sa_master_obj != 0 {
            let sa_master = zap::read_zap(dmu, table, os, sa_master_obj)?;
            let reg_obj = zap::zap_u64(&sa_master, "REGISTRY").unwrap_or(0);
            let lay_obj = zap::zap_u64(&sa_master, "LAYOUTS").unwrap_or(0);
            if reg_obj != 0 {
                let reg = zap::read_zap(dmu, table, os, reg_obj)?;
                for (name, v) in &reg {
                    if let ZapValue::U64(word) = v {
                        // ATTR_ENCODE: num bits 0..16, bswap 16..24,
                        // length bits 24..40.
                        let num = (word & 0xffff) as u16;
                        let length = ((word >> 24) & 0xffff) as u16;
                        attrs.insert(num, (name.clone(), length));
                    }
                }
            }
            if lay_obj != 0 {
                let lay = zap::read_zap(dmu, table, os, lay_obj)?;
                for (key, v) in &lay {
                    if let ZapValue::U16Array(arr) = v
                        && let Ok(num) = key.parse::<u16>()
                    {
                        layouts.insert(num, arr.clone());
                    }
                }
            }
        }
        Ok(SaState { attrs, layouts })
    }

    /// Whether SA metadata is available at all (pre-SA pools use legacy).
    pub fn is_sa(&self) -> bool {
        !self.layouts.is_empty()
    }

    /// Parse a bonus buffer into attrs. SA when the magic matches;
    /// otherwise legacy znode_phys.
    pub fn parse_bonus(&self, bonus: &[u8]) -> ZfsAttrs {
        if bonus.len() >= 8 && u32::from_le_bytes(bonus[0..4].try_into().unwrap()) == SA_MAGIC {
            return self.parse_sa_bonus(bonus);
        }
        parse_legacy_znode(bonus)
    }

    fn parse_sa_bonus(&self, bonus: &[u8]) -> ZfsAttrs {
        let layout_info = u16::from_le_bytes(bonus[4..6].try_into().unwrap());
        let layout = layout_info & 0x3ff;
        let hdrsize = ((layout_info >> 10) as usize) * 8;
        let hdrsize = hdrsize.max(8);
        if bonus.len() < hdrsize {
            return ZfsAttrs::default();
        }
        let Some(order) = self.layouts.get(&layout) else {
            return ZfsAttrs::default();
        };
        // sa_lengths: u16s between the 6-byte fixed header and the data.
        // Number of variable attrs = count of attrs with registered len 0.
        let var_attrs = order
            .iter()
            .filter(|&&num| self.attrs.get(&num).map(|(_, l)| *l == 0).unwrap_or(false))
            .count();
        let lengths_avail = (hdrsize - 6) / 2;
        let lengths: Vec<u16> = (0..lengths_avail.min(var_attrs))
            .map(|i| u16::from_le_bytes(bonus[6 + i * 2..8 + i * 2].try_into().unwrap()))
            .collect();

        let mut cur = hdrsize;
        let mut var_idx = 0usize;
        let mut bytes_for = HashMap::<String, Vec<u8>>::new();
        for &num in order {
            let Some((name, reg_len)) = self.attrs.get(&num) else {
                continue;
            };
            let take: usize = if *reg_len == 0 {
                let v = lengths.get(var_idx).copied().unwrap_or(0) as usize;
                var_idx += 1;
                v
            } else {
                *reg_len as usize
            };
            if cur + take > bonus.len() {
                break; // truncated bonus: keep what we have
            }
            bytes_for.insert(name.clone(), bonus[cur..cur + take].to_vec());
            cur += take;
        }

        let u64_of = |m: &HashMap<String, Vec<u8>>, k: &str| -> Option<u64> {
            m.get(k).and_then(|b| {
                if b.len() >= 8 {
                    Some(u64::from_le_bytes(b[0..8].try_into().unwrap()))
                } else {
                    None
                }
            })
        };
        ZfsAttrs {
            size: u64_of(&bytes_for, "ZPL_SIZE"),
            atime: u64_of(&bytes_for, "ZPL_ATIME"),
            mtime: u64_of(&bytes_for, "ZPL_MTIME"),
            ctime: u64_of(&bytes_for, "ZPL_CTIME"),
            crtime: u64_of(&bytes_for, "ZPL_CRTIME"),
            links: u64_of(&bytes_for, "ZPL_LINKS"),
            parent: u64_of(&bytes_for, "ZPL_PARENT"),
            generation: u64_of(&bytes_for, "ZPL_GEN"),
            mode: u64_of(&bytes_for, "ZPL_MODE"),
            uid: u64_of(&bytes_for, "ZPL_UID"),
            gid: u64_of(&bytes_for, "ZPL_GID"),
        }
    }
}

/// Legacy `znode_phys_t` bonus (`zfs_sa.h` offsets).
pub fn parse_legacy_znode(bonus: &[u8]) -> ZfsAttrs {
    let need = |off: usize, len: usize| off + len <= bonus.len();
    let u64_at = |off: usize| -> Option<u64> {
        if need(off, 8) {
            Some(u64::from_le_bytes(bonus[off..off + 8].try_into().unwrap()))
        } else {
            None
        }
    };
    ZfsAttrs {
        atime: u64_at(0),
        mtime: u64_at(16),
        ctime: u64_at(32),
        crtime: u64_at(48),
        generation: u64_at(64),
        mode: u64_at(72),
        size: u64_at(80),
        parent: u64_at(88),
        links: u64_at(96),
        uid: u64_at(128),
        gid: u64_at(136),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sa_magic_detected() {
        let st = SaState {
            attrs: HashMap::new(),
            layouts: HashMap::new(),
        };
        let mut bonus = vec![0u8; 64];
        bonus[0..4].copy_from_slice(&SA_MAGIC.to_le_bytes());
        // SA path taken (empty registry => default attrs, not legacy).
        let a = st.parse_bonus(&bonus);
        let _ = a;
        // Legacy path when no magic:
        let mut legacy = vec![0u8; 176];
        legacy[80..88].copy_from_slice(&1234u64.to_le_bytes()); // size
        legacy[16..24].copy_from_slice(&999u64.to_le_bytes()); // mtime[0]
        let la = parse_legacy_znode(&legacy);
        assert_eq!(la.size, Some(1234));
        assert_eq!(la.mtime, Some(999));
    }

    #[test]
    fn sa_bonus_layout_walk() {
        // Registry: 0=ATIME(16), 1=MTIME(16), 2=SIZE(8), 3=SYMLINK(var).
        // Layout 7: [ATIME, MTIME, SIZE, SYMLINK] with one var attr.
        let mut attrs = HashMap::new();
        attrs.insert(0u16, ("ZPL_ATIME".to_string(), 16u16));
        attrs.insert(1u16, ("ZPL_MTIME".to_string(), 16u16));
        attrs.insert(2u16, ("ZPL_SIZE".to_string(), 8u16));
        attrs.insert(3u16, ("ZPL_SYMLINK".to_string(), 0u16));
        let mut layouts = HashMap::new();
        layouts.insert(7u16, vec![0u16, 1, 2, 3]);
        let st = SaState { attrs, layouts };

        // hdr: magic(4) + layout_info(2: layout 7 | hdrsize 1*8<<10)
        // + 1 length u16 (for SYMLINK) => hdrsize 8.
        let mut bonus = Vec::new();
        bonus.extend_from_slice(&SA_MAGIC.to_le_bytes());
        bonus.extend_from_slice(&(7u16 | (1u16 << 10)).to_le_bytes());
        bonus.extend_from_slice(&5u16.to_le_bytes()); // SYMLINK length 5
        bonus.extend_from_slice(&1111u64.to_le_bytes()); // ATIME t0
        bonus.extend_from_slice(&2222u64.to_le_bytes()); // ATIME t1
        bonus.extend_from_slice(&3333u64.to_le_bytes()); // MTIME t0
        bonus.extend_from_slice(&4444u64.to_le_bytes()); // MTIME t1
        bonus.extend_from_slice(&777u64.to_le_bytes()); // SIZE
        bonus.extend_from_slice(b"readm"); // SYMLINK payload
        let a = st.parse_bonus(&bonus);
        assert_eq!(a.atime, Some(1111));
        assert_eq!(a.mtime, Some(3333));
        assert_eq!(a.size, Some(777));
    }
}
