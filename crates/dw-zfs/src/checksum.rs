//! zio checksums needed by the scanner: fletcher2, fletcher4 (native and
//! byteswapped word order), and SHA-256 (`sha2`). Verification follows the
//! `blk_cksum` in the block pointer; the label nvlist carries its own
//! fletcher4 in `zio_eck_t`.

use crate::blkptr::BlkPtr;
use crate::error::{ZfsError, ZfsResult};
use sha2::{Digest, Sha256};

/// `zio_cksum_t`: four u64 words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZioCksum(pub [u64; 4]);

impl ZioCksum {
    pub fn from_slice(buf: &[u8]) -> ZioCksum {
        let mut w = [0u64; 4];
        for i in 0..4 {
            w[i] = u64::from_le_bytes(buf[i * 8..i * 8 + 8].try_into().unwrap());
        }
        ZioCksum(w)
    }
}

/// fletcher2 over u64 words: a = sum(w), b = sum(a_i).
pub fn fletcher2(words: &[u8]) -> ZioCksum {
    let mut a: u64 = 0;
    let mut b: u64 = 0;
    let mut i = 0;
    while i + 8 <= words.len() {
        let w = u64::from_le_bytes(words[i..i + 8].try_into().unwrap());
        a = a.wrapping_add(w);
        b = b.wrapping_add(a);
        i += 8;
    }
    ZioCksum([a, b, 0, 0])
}

/// fletcher4 over u64 words: a += w; b += a; c += b; d += c.
/// `swap` processes the same bytes as big-endian words (the
/// ZIO_CHECKSUM_BYTESWAP variant), which covers pools written by
/// big-endian hosts.
pub fn fletcher4(words: &[u8], swap: bool) -> ZioCksum {
    let mut a: u64 = 0;
    let mut b: u64 = 0;
    let mut c: u64 = 0;
    let mut d: u64 = 0;
    let mut i = 0;
    while i + 8 <= words.len() {
        let w = if swap {
            u64::from_be_bytes(words[i..i + 8].try_into().unwrap())
        } else {
            u64::from_le_bytes(words[i..i + 8].try_into().unwrap())
        };
        a = a.wrapping_add(w);
        b = b.wrapping_add(a);
        c = c.wrapping_add(b);
        d = d.wrapping_add(c);
        i += 8;
    }
    ZioCksum([a, b, c, d])
}

/// The label nvlist checksum: fletcher4 over the nvlist region *including*
/// everything before the trailing zio_eck_t (`vdev_phys_t`:
/// `vp_nvlist[VDEV_PHYS_SIZE - 40]` + `zio_eck_t{u64 pad; zio_cksum_t}`).
pub fn label_fletcher4(nvlist_region: &[u8]) -> ZioCksum {
    let eck_at = nvlist_region.len().saturating_sub(40);
    fletcher4(&nvlist_region[..eck_at], false)
}

fn sha256_cksum(data: &[u8]) -> ZioCksum {
    let digest = Sha256::digest(data);
    ZioCksum::from_slice(&digest)
}

/// Verify a data block against its block pointer checksum.
/// `off` is only used for error reporting.
pub fn verify_block(bp: &BlkPtr, off: u64, data: &[u8]) -> ZfsResult<()> {
    let expected = ZioCksum::from_slice(&bp.cksum_bytes());
    let ok = match bp.checksum() {
        crate::blkptr::cksum::FLETCHER_2 => fletcher2(data) == expected,
        crate::blkptr::cksum::FLETCHER_4 => {
            fletcher4(data, false) == expected || fletcher4(data, true) == expected
        }
        crate::blkptr::cksum::SHA256 => sha256_cksum(data) == expected,
        _ => true, // OFF / unsupported: no verification possible
    };
    if ok {
        Ok(())
    } else {
        Err(ZfsError::BlockChecksumMismatch {
            off,
            algo: match bp.checksum() {
                crate::blkptr::cksum::FLETCHER_2 => "fletcher2",
                crate::blkptr::cksum::FLETCHER_4 => "fletcher4",
                crate::blkptr::cksum::SHA256 => "sha256",
                _ => "off",
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fletcher4_known_vector() {
        // 16 bytes = 2 words; computed by hand:
        // w0=1, w1=2 (LE)
        let data = 1u64
            .to_le_bytes()
            .iter()
            .copied()
            .chain(2u64.to_le_bytes().iter().copied())
            .collect::<Vec<u8>>();
        let c = fletcher4(&data, false);
        // a = 1+2 = 3
        // after w1: a=3, b: 1 then 1+3=4, c: 1 then 1+4=5, d: 1 then 1+5=6
        assert_eq!(c, ZioCksum([3, 4, 5, 6]));
    }

    #[test]
    fn fletcher2_known_vector() {
        let data = 1u64
            .to_le_bytes()
            .iter()
            .copied()
            .chain(2u64.to_le_bytes().iter().copied())
            .collect::<Vec<u8>>();
        let c = fletcher2(&data);
        assert_eq!(c, ZioCksum([3, 4, 0, 0]));
    }

    #[test]
    fn sha256_of_empty() {
        // SHA-256("") digest: e3b0c442 98fc1c14 9afbf4c8 996fb924 ...
        // c.0[0] = LE u64 of bytes 0..8 = 0x141cfc9842c4b0e3
        // c.0[1] = LE u64 of bytes 8..16 = 0x24b96f99c8f4fb9a
        let c = sha256_cksum(&[]);
        assert_eq!(c.0[0], 0x141cfc98_42c4b0e3);
        assert_eq!(c.0[1], 0x24b96f99_c8f4fb9a);
    }

    #[test]
    fn label_cksum_excludes_eck() {
        // 48-byte region: 8 data + 40 eck
        let mut region = vec![0u8; 48];
        region[0..8].copy_from_slice(&42u64.to_le_bytes());
        // eck bytes must not influence the checksum
        let mut other = region.clone();
        other[8] = 0xff;
        assert_eq!(label_fletcher4(&region), label_fletcher4(&other));
        let c = label_fletcher4(&region);
        assert_eq!(c, fletcher4(&region[..8], false));
    }
}
