//! blkptr_t / DVA accessors.
//!
//! On-disk layout (128 bytes, from `spa.h`; all bit positions extracted
//! from the BF64 macros in `bitops.h`):
//!
//! ```text
//! 0..48    dva[0..3], each 16 bytes:
//!            word0: asize = (bits 0..24 << 9), vdev = bits 32..56
//!            word1: offset = (bits 0..63 << 9); bit 63 = gang flag
//! 48       blk_prop:
//!            lsize   = ((bits 0..16)  + 1) << 9
//!            psize   = ((bits 16..32) + 1) << 9
//!            comp    = bits 32..39   (ZIO_COMPRESS_*: OFF=2, LZJB=3,
//!                                    GZIP_1..9=5..13, ZLE=14, LZ4=15, ZSTD=16)
//!            embedded= bit 39
//!            cksum   = bits 40..48   (OFF=2, LABEL=3, FLETCHER_2=6,
//!                                    FLETCHER_4=7, SHA256=8)
//!            type    = bits 48..56   (DMU_OT_*)
//!            level   = bits 56..61
//!            crypt   = bit 61
//!            dedup   = bit 62
//!            order   = bit 63
//! 56       blk_prop2
//! 64       blk_pad
//! 72..88   blk_birth_word[2]
//! 88       blk_fill
//! 96..128  blk_cksum (zio_cksum_t, 4 x u64)
//! ```

pub const BLKPTR_SIZE: usize = 128;

/// ZIO compression algorithms (subset the engine decompresses).
pub mod compress {
    pub const OFF: u8 = 2;
    pub const LZJB: u8 = 3;
    pub const EMPTY: u8 = 4;
    pub const GZIP_1: u8 = 5;
    pub const GZIP_9: u8 = 13;
    pub const ZLE: u8 = 14;
    pub const LZ4: u8 = 15;
    pub const ZSTD: u8 = 16;
}

/// ZIO checksum algorithms (subset the engine verifies).
pub mod cksum {
    pub const OFF: u8 = 2;
    pub const LABEL: u8 = 3;
    pub const FLETCHER_2: u8 = 6;
    pub const FLETCHER_4: u8 = 7;
    pub const SHA256: u8 = 8;
}

/// DMU object types the walker dispatches on (`dmu.h` enum order).
pub mod dmu_ot {
    pub const NONE: u8 = 0;
    pub const PACKED_NVLIST: u8 = 3;
    pub const DNODE: u8 = 10;
    pub const OBJSET: u8 = 11;
    pub const DSL_DIR: u8 = 12;
    pub const DSL_DATASET: u8 = 16;
    pub const ZNODE: u8 = 17;
    pub const PLAIN_FILE_CONTENTS: u8 = 19;
    pub const DIRECTORY_CONTENTS: u8 = 20;
    pub const MASTER_NODE: u8 = 21;
    pub const UNLINKED_SET: u8 = 22;
    pub const ZAP_OTHER: u8 = 27;
    pub const SA: u8 = 44;
    pub const SA_MASTER_NODE: u8 = 45;
    pub const SA_ATTR_REGISTRATION: u8 = 46;
    pub const SA_ATTR_LAYOUTS: u8 = 47;
    pub const DSL_DS_SNAP_MAP: u8 = 14;
    pub const DSL_DIR_CHILD_MAP: u8 = 13;
}

pub fn le_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().expect("8-byte slice"))
}

/// A copy of one on-disk blkptr with typed accessors.
#[derive(Debug, Clone)]
pub struct BlkPtr {
    raw: [u8; BLKPTR_SIZE],
}

impl BlkPtr {
    pub fn from_slice(buf: &[u8]) -> Option<BlkPtr> {
        if buf.len() < BLKPTR_SIZE {
            return None;
        }
        let mut raw = [0u8; BLKPTR_SIZE];
        raw.copy_from_slice(&buf[..BLKPTR_SIZE]);
        Some(BlkPtr { raw })
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    fn word(&self, idx: usize) -> u64 {
        le_u64(&self.raw, idx * 8)
    }

    pub fn prop(&self) -> u64 {
        self.word(6)
    }

    // --- sizes ---
    pub fn lsize(&self) -> u64 {
        ((self.prop() & 0xffff) + 1) << 9
    }
    pub fn psize(&self) -> u64 {
        (((self.prop() >> 16) & 0xffff) + 1) << 9
    }

    // --- properties ---
    pub fn compression(&self) -> u8 {
        ((self.prop() >> 32) & 0x7f) as u8
    }
    pub fn is_embedded(&self) -> bool {
        (self.prop() >> 39) & 1 == 1
    }
    pub fn checksum(&self) -> u8 {
        ((self.prop() >> 40) & 0xff) as u8
    }
    pub fn ot_type(&self) -> u8 {
        ((self.prop() >> 48) & 0xff) as u8
    }
    pub fn level(&self) -> u8 {
        ((self.prop() >> 56) & 0x1f) as u8
    }
    pub fn uses_crypt(&self) -> bool {
        (self.prop() >> 61) & 1 == 1
    }

    /// A hole: all DVAs zeroed (`BP_IS_HOLE`).
    pub fn is_hole(&self) -> bool {
        self.word(0) == 0
            && self.word(1) == 0
            && self.word(2) == 0
            && self.word(3) == 0
            && self.word(4) == 0
            && self.word(5) == 0
    }

    // --- DVAs ---
    pub fn ndva(&self) -> usize {
        for i in 0..3 {
            if self.dva_offset(i as u8).is_none() {
                return i;
            }
        }
        3
    }

    /// DVA `idx` (0-based). Returns None when the DVA slot is empty.
    pub fn dva(&self, idx: u8) -> Option<Dva> {
        if idx as usize >= 3 {
            return None;
        }
        let w0 = self.word(idx as usize * 2);
        let w1 = self.word(idx as usize * 2 + 1);
        if w0 == 0 && w1 == 0 {
            return None;
        }
        Some(Dva {
            asize: (w0 & 0x00ff_ffff) << 9,
            vdev: (w0 >> 32) & 0x00ff_ffff,
            offset: (w1 & 0x7fff_ffff_ffff_ffff) << 9,
            gang: (w1 >> 63) & 1 == 1,
        })
    }

    fn dva_offset(&self, idx: u8) -> Option<u64> {
        self.dva(idx).map(|d| d.offset)
    }

    /// Birth txg: `blk_birth_word[1]` (logical birth).
    pub fn birth_txg(&self) -> u64 {
        self.word(10)
    }

    pub fn cksum_bytes(&self) -> [u8; 32] {
        self.raw[96..128].try_into().expect("blk_cksum is 32 bytes")
    }
}

/// One Data Virtual Address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dva {
    /// Allocated size on the vdev in bytes (always sector-multiples).
    pub asize: u64,
    pub vdev: u64,
    /// Byte offset on the vdev.
    pub offset: u64,
    pub gang: bool,
}

impl Dva {
    /// The physical byte range this DVA occupies.
    pub fn range(&self) -> std::ops::Range<u64> {
        self.offset..self.offset + self.asize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bp_with_prop(prop: u64) -> BlkPtr {
        let mut raw = [0u8; BLKPTR_SIZE];
        raw[48..56].copy_from_slice(&prop.to_le_bytes());
        BlkPtr::from_slice(&raw).unwrap()
    }

    #[test]
    fn sizes_decode_with_bias() {
        // lsize 4096 => (4096>>9)-1 = 7 in the low 16 bits; psize 512
        // stores the bias-0 value (512/512-1 = 0).
        let lsize_bits: u64 = (4096 / 512 - 1) & 0xffff; // 7
        let psize_bits: u64 = 0u64;
        let bp = bp_with_prop(lsize_bits | psize_bits);
        assert_eq!(bp.lsize(), 4096);
        assert_eq!(bp.psize(), 512);
    }

    #[test]
    fn prop_fields_decode() {
        // comp=LZ4(15) bits32..39, cksum=FLETCHER_4(7) bits40..48,
        // type=PLAIN_FILE_CONTENTS(19) bits48..56, level=0
        let prop = (15u64 << 32) | (7u64 << 40) | (19u64 << 48);
        let bp = bp_with_prop(prop);
        assert_eq!(bp.compression(), 15);
        assert_eq!(bp.checksum(), 7);
        assert_eq!(bp.ot_type(), 19);
        assert_eq!(bp.level(), 0);
        assert!(!bp.is_embedded());
    }

    #[test]
    fn dva_decode() {
        let mut raw = [0u8; BLKPTR_SIZE];
        // DVA0 word0: asize field (as 512-multiples: 0x200 => asize 0x40000)
        let asize_units: u64 = 512; // -> 256 KiB
        let vdev: u64 = 7 << 32;
        raw[0..8].copy_from_slice(&(asize_units | vdev).to_le_bytes());
        // DVA0 word1: offset field in 512-multiples: 1024 -> 512 KiB
        raw[8..16].copy_from_slice(&1024u64.to_le_bytes());
        let bp = BlkPtr::from_slice(&raw).unwrap();
        let d = bp.dva(0).unwrap();
        assert_eq!(d.asize, 512 * 512);
        assert_eq!(d.vdev, 7);
        assert_eq!(d.offset, 1024 * 512);
        assert!(!d.gang);
        assert_eq!(d.range(), 512 * 1024..512 * 1024 + 512 * 512);
        assert!(bp.dva(1).is_none());
        assert!(bp.dva(2).is_none());
    }

    #[test]
    fn hole_detection() {
        let raw = [0u8; BLKPTR_SIZE];
        let bp = BlkPtr::from_slice(&raw).unwrap();
        assert!(bp.is_hole());
        let mut raw2 = [0u8; BLKPTR_SIZE];
        raw2[0] = 1; // any nonzero word0
        let bp2 = BlkPtr::from_slice(&raw2).unwrap();
        assert!(!bp2.is_hole());
    }

    #[test]
    fn embedded_flag() {
        let prop = 1u64 << 39;
        let bp = bp_with_prop(prop);
        assert!(bp.is_embedded());
    }
}
