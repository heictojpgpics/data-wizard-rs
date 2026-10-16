//! Page scan: locate "MSB+" metadata pages (and, for the deep pass,
//! orphan page copies that no longer carry a page header) across the
//! volume, cluster-aligned.
//!
//! Page header (validated on a real volume):
//! ```text
//! +0x00  "MSB+"
//! +0x04  u32  (2 on every page observed across ReFS 3.4 - 3.14)
//! +0x0c  u32  per-volume constant shared with the superblock
//! +0x20..0x40  u64 x4: the page's address quadruple
//! ```

pub const MAGIC_MSB: &[u8; 4] = b"MSB+";

/// One located page.
#[derive(Debug, Clone)]
pub struct PageHit {
    /// Byte offset of the page (cluster-aligned).
    pub offset: u64,
    /// True when the page carries an MSB+ header.
    pub headed: bool,
}

/// Scan `buf` (a window of the volume starting at `win_off`) for page
/// candidates at `cluster` alignment: headed pages by magic, and
/// headless (orphan) windows by record signatures found there by the
/// caller. This function only finds the headed ones; the walker also
/// feeds headless windows by scanning every cluster that contains
/// record-like data.
pub fn find_headed_pages(buf: &[u8], win_off: u64, cluster: u64, out: &mut Vec<PageHit>) {
    let mut i = 0;
    while i + 4 <= buf.len() {
        if &buf[i..i + 4] == MAGIC_MSB {
            out.push(PageHit {
                offset: win_off + i as u64,
                headed: true,
            });
        }
        i += cluster as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_aligned_pages() {
        let mut buf = vec![0u8; 0x8000];
        buf[0x4000..0x4004].copy_from_slice(MAGIC_MSB);
        buf[0x6000..0x6004].copy_from_slice(MAGIC_MSB); // NOT cluster aligned (4K grid: ok at 0x6000)
        let mut out = Vec::new();
        find_headed_pages(&buf, 0x10000, 4096, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].offset, 0x14000);
        assert_eq!(out[1].offset, 0x16000);
    }

    #[test]
    fn ignores_unaligned_magic() {
        let mut buf = vec![0u8; 0x4000];
        buf[0x2001..0x2005].copy_from_slice(MAGIC_MSB); // off-grid
        let mut out = Vec::new();
        find_headed_pages(&buf, 0, 4096, &mut out);
        assert!(out.is_empty());
    }
}
