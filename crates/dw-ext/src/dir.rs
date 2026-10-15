//! Directory entry parsing and the deleted-entry ("shadow dirent") scan.
//!
//! ext2/3/4 dirents: `inode u32 | rec_len u16 | name_len u8 | file_type u8`
//! followed by the name. `rec_len` covers the whole entry (min 8, padded
//! to 4). On unlink, ext does not scrub the entry: the *previous* live
//! entry's rec_len is extended over the removed one, so the stale name
//! bytes remain on disk until the block is reused. A raw walk that also
//! reports entries *shadowed* by the live chain therefore recovers the
//! names of recently deleted files.
//!
//! The shadow scan never invents bytes: an entry is reported only when
//! the raw block contains a structurally valid dirent (bounds, printable
//! name, sane type) that the live chain skips.

/// One parsed raw dirent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDirent {
    pub inode: u32,
    /// Byte offset of this entry within the directory block.
    pub offset_in_block: usize,
    pub rec_len: u16,
    pub name_len: u8,
    pub file_type: u8,
    pub name: String,
}

/// Live-chain dirent plus whether the next entries in the block were
/// skipped over (their bytes still sit on disk - shadow scan territory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkOutcome {
    Live(RawDirent),
    EndOfBlock,
}

/// Maximum entries per block (rec_len >= 8 => block_size/8).
const MAX_DIRENTS_PER_BLOCK: usize = 4096 / 8;

/// Walk one directory block's LIVE chain (the entries reachable by
/// rec_len hops). `block` is one fs block of directory data.
pub fn live_chain(block: &[u8]) -> Vec<RawDirent> {
    let mut out = Vec::new();
    let mut off = 0usize;
    let mut hops = 0usize;
    while off + 8 <= block.len() {
        hops += 1;
        if hops > MAX_DIRENTS_PER_BLOCK {
            break;
        }
        let inode =
            u32::from_le_bytes([block[off], block[off + 1], block[off + 2], block[off + 3]]);
        let rec_len = u16::from_le_bytes([block[off + 4], block[off + 5]]);
        let name_len = block[off + 6];
        let file_type = block[off + 7];
        if rec_len < 8 || off + usize::from(rec_len) > block.len() {
            break; // corrupt chain; stop (caller reports partial dir)
        }
        let name = latin1_name(&block[off + 8..block.len()], name_len as usize);
        if inode != 0 && name_len > 0 {
            out.push(RawDirent {
                inode,
                offset_in_block: off,
                rec_len,
                name_len,
                file_type,
                name,
            });
        }
        off += usize::from(rec_len);
    }
    out
}

/// Scan one directory block for structurally valid dirents that the live
/// chain does NOT reach. Returns them in block order.
pub fn shadowed_entries(block: &[u8], live: &[RawDirent]) -> Vec<RawDirent> {
    let live_offsets: std::collections::HashSet<usize> =
        live.iter().map(|d| d.offset_in_block).collect();
    let mut out = Vec::new();
    let mut off = 0usize;
    let mut hops = 0usize;
    while off + 8 <= block.len() {
        hops += 1;
        if hops > MAX_DIRENTS_PER_BLOCK {
            break;
        }
        let inode =
            u32::from_le_bytes([block[off], block[off + 1], block[off + 2], block[off + 3]]);
        let rec_len = u16::from_le_bytes([block[off + 4], block[off + 5]]);
        let name_len = block[off + 6];
        let file_type = block[off + 7];

        // Advance by a structurally valid step: a plausible entry has
        // rec_len in 8..=block_len, multiple of 4, name_len <= rec_len-8.
        if rec_len < 8
            || !rec_len.is_multiple_of(4)
            || off + usize::from(rec_len) > block.len()
            || usize::from(name_len) + 8 > usize::from(rec_len)
        {
            off += 4;
            continue;
        }
        let name = latin1_name(
            &block[off + 8..off + 8 + name_len as usize],
            name_len as usize,
        );
        let plausible = inode != 0
            && name_len > 0
            && file_type <= 7
            && is_plausible_name(&name)
            && name != "."
            && name != "..";
        if plausible && !live_offsets.contains(&off) {
            out.push(RawDirent {
                inode,
                offset_in_block: off,
                rec_len,
                name_len,
                file_type,
                name,
            });
        }
        off += 4; // dense scan: next candidate at +4 (rec_len alignment)
    }
    out
}

/// ext dirent names are raw bytes; we accept printable latin-1 (ASCII
/// range plus accented chars) and reject control bytes.
fn is_plausible_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 255 && s.bytes().all(|b| b >= 0x20 && b != 0x7f)
}

fn latin1_name(raw: &[u8], len: usize) -> String {
    let end = len.min(raw.len());
    String::from_utf8_lossy(&raw[..end]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(inode: u32, name: &str, rec_len: u16, file_type: u8) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&inode.to_le_bytes());
        e.extend_from_slice(&rec_len.to_le_bytes());
        e.push(name.len() as u8);
        e.push(file_type);
        let mut padded = name.as_bytes().to_vec();
        padded.resize(usize::from(rec_len) - 8, 0);
        e.extend_from_slice(&padded);
        e
    }

    /// Block: ".", "..", "hello.txt"(rec 32) [shadow "gone.txt"(32)], "tail.bin"(20->24 pad)
    fn sample_block() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&entry(2, ".", 12, 2));
        b.extend_from_slice(&entry(2, "..", 12, 2));
        // hello.txt entry covers itself + shadowed gone.txt (rec_len 64):
        let mut e = entry(3, "hello.txt", 64, 1);
        // shadowed entry bytes placed inside hello.txt's slack:
        let sh = entry(4, "gone.txt", 32, 1);
        e[32..32 + sh.len()].copy_from_slice(&sh);
        b.extend_from_slice(&e);
        b.extend_from_slice(&entry(7, "tail.bin", 24, 1));
        while b.len() < 4096 {
            b.push(0);
        }
        b
    }

    #[test]
    fn live_chain_skips_shadow() {
        let block = sample_block();
        let live = live_chain(&block);
        let names: Vec<&str> = live.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec![".", "..", "hello.txt", "tail.bin"]);
    }

    #[test]
    fn shadow_scan_finds_gone_txt() {
        let block = sample_block();
        let live = live_chain(&block);
        let shadows = shadowed_entries(&block, &live);
        let names: Vec<&str> = shadows.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["gone.txt"]);
        assert_eq!(shadows[0].inode, 4);
        assert_eq!(shadows[0].file_type, 1);
    }

    #[test]
    fn corrupt_rec_len_stops_chain() {
        let mut block = sample_block();
        // hello.txt entry at offset 24: corrupt its rec_len to 4 (<8).
        block[24 + 4] = 4;
        block[24 + 5] = 0;
        let live = live_chain(&block);
        let names: Vec<&str> = live.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec![".", ".."]); // chain stopped at corrupt entry
    }

    #[test]
    fn garbage_bytes_produce_no_shadows() {
        let block = vec![0x41u8; 4096]; // "AAAA..." - names plausible but inode 0
        let live = live_chain(&block);
        assert!(live.is_empty());
        let shadows = shadowed_entries(&block, &live);
        assert!(shadows.is_empty()); // inode==0 everywhere
    }

    #[test]
    fn dot_entries_never_shadow() {
        // A stray "." that the live chain skips must not be reported
        // (it would duplicate the directory itself).
        let mut block = Vec::new();
        block.extend_from_slice(&entry(2, ".", 12, 2));
        block.extend_from_slice(&entry(2, "..", 12, 2));
        // live entry whose slack contains a shadowed "."
        let mut e = entry(9, "real.txt", 64, 1);
        let sh = entry(9, ".", 32, 2);
        e[32..32 + sh.len()].copy_from_slice(&sh);
        block.extend_from_slice(&e);
        while block.len() < 4096 {
            block.push(0);
        }
        let live = live_chain(&block);
        let shadows = shadowed_entries(&block, &live);
        assert!(shadows.is_empty());
    }
}
