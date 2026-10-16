//! ZAP (ZFS Attribute Processor) directory parsing: microzap (bonus-resident
//! small directories) and fatzap (block-backed large directories), from
//! `zap_impl.h` / `zap_leaf.h`.
//!
//! Constants (extracted):
//! - `ZBT_MICRO = (1<<63)+3`, `ZBT_HEADER = (1<<63)+1`, `ZBT_LEAF = (1<<63)+0`
//! - `ZAP_MAGIC = 0x2F52AB2AB`
//! - microzap: `mz_block_type`@0, `mz_salt`@8, `mz_normflags`@16,
//!   `mz_pad[5]`@24, entries from @64, each 64 B:
//!   `mze_value` u64@0, `mze_cd` u32@8, `mze_pad` u16@12, `mze_name[50]`@14.
//! - fatzap header: ptrtbl zt_blk@16 zt_numblks@24 zt_shift@32 zt_nextblk@40
//!   zt_blks_copied@48; `zap_freeblk`@56 `zap_num_leafs`@64 `zap_num_entries`@72
//!   `zap_salt`@80; embedded ptr table: second half of the block, entry i at
//!   `blocksize/2 + i*8` (`ZAP_EMBEDDED_PTRTBL_SHIFT = bs - 4`).
//! - leaf: header 48 B (`lh_magic` u32@24 = 0x2AB1EAF, `lh_nentries` u16@30),
//!   then the hash table (u16 * 2^(bs-5)), then 24-B chunks (638 for 16 KiB);
//!   entry chunk: `le_type`@0 (252), `le_value_intlen`@1, `le_next` u16@2,
//!   `le_name_chunk` u16@4, `le_name_numints` u16@6, `le_value_chunk` u16@8,
//!   `le_value_numints` u16@10, `le_cd` u32@12, `le_hash` u64@16.
//!   array chunk: `la_type`@0 (251), `la_array[21]`@1, `la_next` u16@22.

use crate::blkptr::BlkPtr;
use crate::dmu::{Dmu, Dnode, ObjectTable, Objset};
use crate::error::{ZfsError, ZfsResult};

pub const ZBT_MICRO: u64 = (1u64 << 63) + 3;
pub const ZBT_HEADER: u64 = (1u64 << 63) + 1;
pub const ZBT_LEAF: u64 = 1u64 << 63;
pub const ZAP_MAGIC: u64 = 0x0002_F52A_B2AB;
pub const ZAP_LEAF_MAGIC: u32 = 0x2AB_1EAF;
pub const MZAP_ENT_LEN: usize = 64;
pub const ZAP_CHUNK_ENTRY: u8 = 252;
pub const ZAP_CHUNK_ARRAY: u8 = 251;
pub const ZAP_LEAF_CHUNKSIZE: usize = 24;
/// fzap default block shift (`zap_fat.c`).
pub const FZAP_BLOCK_SHIFT: u8 = 14;

/// One decoded zap entry. Values are either a u64 (object ids, registry
/// words) or a u16 array (layout attribute lists).
#[derive(Debug, Clone, PartialEq)]
pub enum ZapValue {
    U64(u64),
    U16Array(Vec<u16>),
}

/// Read object `objid` (of a given objset) as a zap and return its entries.
/// Micro and fat zaps are both handled.
pub fn read_zap(
    dmu: &mut Dmu,
    table: &mut ObjectTable,
    os: &Objset,
    objid: u64,
) -> ZfsResult<Vec<(String, ZapValue)>> {
    let dn = table.dnode(dmu, os, objid)?.ok_or_else(|| ZfsError::Zap {
        obj: objid,
        reason: "object missing".into(),
    })?;
    read_zap_dnode(dmu, &dn, objid)
}

/// Read a zap given its dnode directly (block 0 of its data).
pub fn read_zap_dnode(dmu: &mut Dmu, dn: &Dnode, objid: u64) -> ZfsResult<Vec<(String, ZapValue)>> {
    let block = dmu.read_dnode_block(dn, 0)?.ok_or_else(|| ZfsError::Zap {
        obj: objid,
        reason: "no data block".into(),
    })?;
    let block_type = u64::from_le_bytes(block[0..8].try_into().unwrap());
    match block_type {
        ZBT_MICRO => parse_microzap(&block, objid),
        ZBT_HEADER => parse_fatzap(dmu, dn, &block, objid),
        other => Err(ZfsError::Zap {
            obj: objid,
            reason: format!("unknown zap block type {other}"),
        }),
    }
}

fn parse_microzap(block: &[u8], objid: u64) -> ZfsResult<Vec<(String, ZapValue)>> {
    // Block must be at least the mzap header (64 B) + one entry.
    if block.len() < 64 + MZAP_ENT_LEN {
        return Err(ZfsError::Zap {
            obj: objid,
            reason: "microzap too small".into(),
        });
    }
    let salt = u64::from_le_bytes(block[8..16].try_into().unwrap());
    let _ = salt;
    let mut out = Vec::new();
    let mut off = 64;
    while off + MZAP_ENT_LEN <= block.len() {
        let ent = &block[off..off + MZAP_ENT_LEN];
        let value = u64::from_le_bytes(ent[0..8].try_into().unwrap());
        // cd = u32 @8; pad u16 @12; name[50] @14 (NUL-terminated)
        let name_raw: Vec<u8> = ent[14..64]
            .iter()
            .copied()
            .take_while(|&b| b != 0)
            .collect();
        if name_raw.is_empty() {
            off += MZAP_ENT_LEN;
            continue; // free slot
        }
        let name = match String::from_utf8(name_raw) {
            Ok(n) => n,
            Err(_) => continue, // corrupt slot: skip, do not abort the dir
        };
        out.push((name, ZapValue::U64(value)));
        off += MZAP_ENT_LEN;
        if out.len() > 8192 {
            return Err(ZfsError::Zap {
                obj: objid,
                reason: "too many microzap entries".into(),
            });
        }
    }
    Ok(out)
}

fn parse_fatzap(
    dmu: &mut Dmu,
    dn: &Dnode,
    header: &[u8],
    objid: u64,
) -> ZfsResult<Vec<(String, ZapValue)>> {
    if header.len() < 128 {
        return Err(ZfsError::Zap {
            obj: objid,
            reason: "fatzap header too small".into(),
        });
    }
    let magic = u64::from_le_bytes(header[8..16].try_into().unwrap());
    if magic != ZAP_MAGIC {
        return Err(ZfsError::Zap {
            obj: objid,
            reason: "fatzap magic mismatch".into(),
        });
    }
    let g = |off: usize| u64::from_le_bytes(header[off..off + 8].try_into().unwrap());
    let zt_blk = g(16);
    let zt_numblks = g(24);
    let zt_shift = g(32);
    let num_leafs = g(64);
    let num_entries = g(72);

    // Resolve the pointer table.
    let blocksize = dn.datablksz();
    if blocksize == 0 {
        return Err(ZfsError::Zap {
            obj: objid,
            reason: "zero blocksize".into(),
        });
    }
    let table_entries = 1usize << zt_shift.min(20) as usize;
    let table: Vec<u64> = if zt_blk == 0 {
        // Embedded: second half of the header block.
        let half = blocksize as usize / 2;
        if header.len() < blocksize as usize {
            return Err(ZfsError::Zap {
                obj: objid,
                reason: "short fatzap block".into(),
            });
        }
        let start = half;
        let mut t = Vec::with_capacity(table_entries);
        for i in 0..table_entries {
            let at = start + i * 8;
            if at + 8 > header.len() {
                return Err(ZfsError::Zap {
                    obj: objid,
                    reason: "embedded ptrtbl out of range".into(),
                });
            }
            t.push(u64::from_le_bytes(header[at..at + 8].try_into().unwrap()));
        }
        t
    } else {
        // External: zt_blk..zt_blk+zt_numblks are whole blocks of u64s.
        let mut t = Vec::with_capacity(table_entries);
        for b in 0..zt_numblks.min(1024) {
            let block = dmu
                .read_dnode_block(dn, zt_blk + b)?
                .ok_or_else(|| ZfsError::Zap {
                    obj: objid,
                    reason: "ptrtbl block missing".into(),
                })?;
            for i in 0..block.len() / 8 {
                t.push(u64::from_le_bytes(
                    block[i * 8..i * 8 + 8].try_into().unwrap(),
                ));
            }
        }
        t
    };

    // Walk every distinct, valid leaf referenced by the table.
    let mut seen: Vec<u64> = Vec::new();
    let mut out = Vec::new();
    let mut emitted = 0usize;
    for &leaf_id in &table {
        if leaf_id == 0 || leaf_id > (1u64 << 40) {
            continue; // free slot (0) or nonsense
        }
        if seen.contains(&leaf_id) {
            continue; // the table repeats leaf ids across hash slots
        }
        seen.push(leaf_id);
        if seen.len() > 4096 {
            return Err(ZfsError::Zap {
                obj: objid,
                reason: "ptrtbl references too many leaves".into(),
            });
        }
        let leaf = match dmu.read_dnode_block(dn, leaf_id)? {
            Some(l) => l,
            None => continue,
        };
        emitted += parse_leaf(&leaf, &mut out, objid)?;
        if emitted > 16384 {
            return Err(ZfsError::Zap {
                obj: objid,
                reason: "too many fatzap entries".into(),
            });
        }
    }

    // Consistency: entries found should approximate zap_num_entries; allow
    // slack for pools mid-transaction, but not silent zero.
    if num_entries > 0 && out.is_empty() {
        // All leaves unreadable or corrupt: report, do not invent data.
        tracing::warn!(
            obj = objid,
            leaves = num_leafs,
            "fatzap: no entries decoded"
        );
    }
    Ok(out)
}

/// Parse one zap leaf; returns the number of entries found.
/// Public for the fuzz targets (pure function over a block).
pub fn parse_leaf(leaf: &[u8], out: &mut Vec<(String, ZapValue)>, objid: u64) -> ZfsResult<usize> {
    // Leaf layout: 48-B header, hash table, chunks.
    const HDR: usize = 48;
    if leaf.len() < HDR {
        return Ok(0);
    }
    let magic = u32::from_le_bytes(leaf[24..28].try_into().unwrap());
    if magic != ZAP_LEAF_MAGIC {
        return Ok(0); // not a leaf (free table slot): skip
    }
    let lh_nentries = u16::from_le_bytes(leaf[30..32].try_into().unwrap()) as usize;

    // Chunk count from the block size (fzap default 16 KiB unless the
    // dnode says otherwise; the leaf's own size drives it here).
    let bs = leaf.len();
    if bs < 128 {
        return Ok(0); // degenerate
    }
    let hash_entries = 1usize << ((bs.trailing_zeros() as usize).saturating_sub(5));
    let chunks_base = HDR + hash_entries * 2;
    let num_chunks = (bs - chunks_base) / ZAP_LEAF_CHUNKSIZE;

    // Chunks array accessor.
    let chunk = |idx: usize| -> Option<&[u8]> {
        let at = chunks_base + idx * ZAP_LEAF_CHUNKSIZE;
        leaf.get(at..at + ZAP_LEAF_CHUNKSIZE)
    };

    // Walk entry chunks in order (chunk 0..num_chunks); entries are
    // distinguished by type. Build name/value from array chunks.
    let mut found = 0usize;
    for idx in 0..num_chunks {
        let c = match chunk(idx) {
            Some(c) => c,
            None => break,
        };
        if c[0] != ZAP_CHUNK_ENTRY {
            continue;
        }
        let value_intlen = c[1] as usize;
        let name_chunk = u16::from_le_bytes(c[4..6].try_into().unwrap()) as usize;
        let name_numints = u16::from_le_bytes(c[6..8].try_into().unwrap()) as usize;
        let value_chunk = u16::from_le_bytes(c[8..10].try_into().unwrap()) as usize;
        let value_numints = u16::from_le_bytes(c[10..12].try_into().unwrap()) as usize;

        // Assemble the name from array chunks (chain via la_next).
        let mut name_bytes = Vec::with_capacity(name_numints * 4);
        let mut cur = name_chunk;
        let mut guard = 0;
        while cur != 0xFFFF && cur < num_chunks {
            guard += 1;
            if guard > 256 {
                break;
            }
            let ac = match chunk(cur) {
                Some(a) => a,
                None => break,
            };
            if ac[0] != ZAP_CHUNK_ARRAY {
                break;
            }
            name_bytes.extend_from_slice(&ac[1..22]);
            let next = u16::from_le_bytes(ac[22..24].try_into().unwrap());
            if next as usize <= cur {
                break; // corrupt chain
            }
            cur = next as usize;
        }
        let name_vec: Vec<u8> = name_bytes.iter().copied().take_while(|&b| b != 0).collect();
        // The chain's last chunk holds the value data for u64 values with
        // intlen 8 and numints 1 (typical directory entry: value in the
        // entry's own value_chunk chain).
        let mut value_bytes = Vec::with_capacity(value_numints * value_intlen.max(1));
        let mut vcur = value_chunk;
        let mut vguard = 0;
        while vcur != 0xFFFF && vcur < num_chunks {
            vguard += 1;
            if vguard > 256 {
                break;
            }
            let ac = match chunk(vcur) {
                Some(a) => a,
                None => break,
            };
            if ac[0] != ZAP_CHUNK_ARRAY {
                break;
            }
            value_bytes.extend_from_slice(&ac[1..22]);
            let next = u16::from_le_bytes(ac[22..24].try_into().unwrap());
            if next as usize <= vcur {
                break;
            }
            vcur = next as usize;
        }

        let name = match String::from_utf8(name_vec) {
            Ok(n) if !n.is_empty() => n,
            _ => continue,
        };
        // The array chunks round every payload up to 21 bytes; the real
        // value is exactly numints * intlen.
        let want = value_numints * value_intlen.max(1);
        if value_bytes.len() > want {
            value_bytes.truncate(want);
        }
        if value_intlen == 8 && value_numints == 1 && value_bytes.len() >= 8 {
            let v = u64::from_le_bytes(value_bytes[0..8].try_into().unwrap());
            out.push((name, ZapValue::U64(v)));
        } else if value_intlen == 2 && value_bytes.len() >= 2 {
            let mut arr = Vec::with_capacity(value_bytes.len() / 2);
            for i in 0..value_bytes.len() / 2 {
                arr.push(u16::from_le_bytes([
                    value_bytes[i * 2],
                    value_bytes[i * 2 + 1],
                ]));
            }
            out.push((name, ZapValue::U16Array(arr)));
        } else {
            // Unknown value shape: keep the key with no value.
            out.push((name, ZapValue::U64(0)));
        }
        found += 1;
        let _ = lh_nentries;
        let _ = objid;
    }
    Ok(found)
}

/// Convenience: fetch a u64 from a zap by key.
pub fn zap_u64(entries: &[(String, ZapValue)], key: &str) -> Option<u64> {
    entries
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| match v {
            ZapValue::U64(x) => Some(*x),
            _ => None,
        })
}

/// Unused-but-part-of-the-API helper for callers that hold a BlkPtr.
pub fn zap_first_block(dn: &Dnode) -> Option<BlkPtr> {
    dn.blkptr(0)
}
