//! XDR-encoded nvlist decoding, the format used by vdev labels and the
//! on-disk pool config (`nvpair.c`, "XDR encoding functions" comment).
//!
//! Stream layout (all fields big-endian? No: the 4-byte header says the
//! encoding method; XDR integers are written in the *writer's* byte order.
//! Labels written on x86 (little-endian) hosts - the overwhelmingly common
//! case - pack every u32/u64 little-endian. The decoder therefore reads the
//! header in both orders and then applies it to the whole stream.
//!
//! ```text
//! packed nvlist:
//!   u32  encoding method + host endian flag (NV_ENCODE_XDR = 1)
//!   u32  nvl_version
//!   u32  nvl_nvflag
//!   nvpair records...
//!   u32 0, u32 0  (end marker: decode_len == 0)
//!
//! nvpair record:
//!   u32  encoded_size
//!   u32  decoded_size  (0 => end of list)
//!   name:   u32 strlen, bytes + NUL, padded to 4
//!   u32  data type
//!   u32  nelem
//!   value (type-specific; strings: u32 len + bytes + NUL + pad;
//!          nested nvlists: a full packed nvlist incl. its own header)
//! ```

use crate::error::{ZfsError, ZfsResult};

pub const NV_ENCODE_NATIVE: u32 = 0;
pub const NV_ENCODE_XDR: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum NvValue {
    Boolean,
    U64(u64),
    String(String),
    NvList(NvList),
    /// Nested nvlist array (`children` of a vdev tree, etc.).
    NvListArray(Vec<NvList>),
    /// Anything the scanner does not need; kept with its length so the
    /// record is still validated for size consistency.
    Other {
        type_id: u32,
        nelem: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NvList {
    pairs: Vec<(String, NvValue)>,
}

/// Byte order of the stream, decided from the first header word.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Order {
    Le,
    Be,
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    order: Order,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Option<u32> {
        let b = self.buf.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(match self.order {
            Order::Le => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Order::Be => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    fn u64(&mut self) -> Option<u64> {
        let b = self.buf.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        Some(match self.order {
            Order::Le => u64::from_le_bytes(b.try_into().ok()?),
            Order::Be => u64::from_be_bytes(b.try_into().ok()?),
        })
    }

    /// XDR string: u32 length (non-NUL bytes), data + NUL, padded to 4.
    fn xdr_string(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        if len > 4096 {
            return None;
        }
        let padded = (len + 3) & !3;
        let raw = self.buf.get(self.pos..self.pos + padded)?;
        self.pos += padded;
        let bytes: Vec<u8> = raw.iter().copied().take(len).collect();
        String::from_utf8(bytes).ok()
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.buf.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(())
    }
}

/// Data type ids (`nvpair.h` `data_type_t`).
mod types {
    pub const BOOLEAN: u32 = 1;
    pub const BYTE: u32 = 2;
    pub const INT16: u32 = 3;
    pub const UINT16: u32 = 4;
    pub const INT32: u32 = 5;
    pub const UINT32: u32 = 6;
    pub const INT64: u32 = 7;
    pub const UINT64: u32 = 8;
    pub const STRING: u32 = 9;
    pub const BYTE_ARRAY: u32 = 10;
    pub const UINT64_ARRAY: u32 = 16;
    pub const STRING_ARRAY: u32 = 17;
    pub const NVLIST: u32 = 19;
    pub const NVLIST_ARRAY: u32 = 20;
    pub const BOOLEAN_VALUE: u32 = 21;
}

impl NvList {
    /// Parse a packed XDR nvlist. Accepts NV_VERSION 0/1 and both host
    /// byte orders; rejects anything that does not carry the XDR marker.
    pub fn parse(buf: &[u8]) -> ZfsResult<NvList> {
        if buf.len() < 12 {
            return Err(ZfsError::Corrupt("nvlist: shorter than header".into()));
        }
        let le_head = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let be_head = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        // The method word is NV_ENCODE_XDR (1) or NATIVE (0) with the
        // host-endian flag in the top bits for native streams; XDR streams
        // on LE hosts are written with xdrmem, so the 4-byte words are LE.
        let order = if le_head & 0xff_ffff == NV_ENCODE_XDR || le_head == NV_ENCODE_NATIVE {
            Order::Le
        } else if be_head & 0xff_ffff == NV_ENCODE_XDR || be_head == NV_ENCODE_NATIVE {
            Order::Be
        } else {
            return Err(ZfsError::Corrupt(format!(
                "nvlist: header method word 0x{le_head:08x} is neither XDR nor NATIVE"
            )));
        };

        let mut r = Reader { buf, pos: 0, order };
        // header: method, version, nvflag
        r.u32()
            .ok_or_else(|| ZfsError::Corrupt("nvlist: hdr0".into()))?;
        let version = r
            .u32()
            .ok_or_else(|| ZfsError::Corrupt("nvlist: hdr1".into()))?;
        if version > 1 {
            return Err(ZfsError::Corrupt(format!(
                "nvlist: unsupported version {version}"
            )));
        }
        r.u32()
            .ok_or_else(|| ZfsError::Corrupt("nvlist: hdr2".into()))?;

        let mut out = NvList::default();
        loop {
            // nvpair record header
            let _encoded_size = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: pair hdr0".into()))?;
            let decoded_size = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: pair hdr1".into()))?;
            if decoded_size == 0 {
                break; // end-of-list marker
            }
            let name = r
                .xdr_string()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: pair name".into()))?;
            let type_id = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: pair type".into()))?;
            let nelem = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: pair nelem".into()))?;

            let value = match type_id {
                types::BOOLEAN | types::BOOLEAN_VALUE => {
                    if type_id == types::BOOLEAN_VALUE {
                        r.skip(4);
                    }
                    NvValue::Boolean
                }
                types::BYTE | types::INT16 | types::UINT16 | types::INT32 | types::UINT32 => {
                    r.skip(4 * nelem.max(1) as usize);
                    NvValue::Other { type_id, nelem }
                }
                types::INT64 | types::UINT64 => {
                    let v = r.u64().unwrap_or(0);
                    NvValue::U64(v)
                }
                types::STRING => {
                    let s = r
                        .xdr_string()
                        .ok_or_else(|| ZfsError::Corrupt("nvlist: string value".into()))?;
                    NvValue::String(s)
                }
                types::BYTE_ARRAY => {
                    r.skip(((nelem as usize) + 3) & !3);
                    NvValue::Other { type_id, nelem }
                }
                types::UINT64_ARRAY => {
                    r.skip(4 + 8 * nelem as usize);
                    NvValue::Other { type_id, nelem }
                }
                types::STRING_ARRAY => {
                    for _ in 0..nelem {
                        r.xdr_string()
                            .ok_or_else(|| ZfsError::Corrupt("nvlist: string array".into()))?;
                    }
                    NvValue::Other { type_id, nelem }
                }
                types::NVLIST => {
                    let nested = Self::parse_at(&mut r)?;
                    NvValue::NvList(nested)
                }
                types::NVLIST_ARRAY => {
                    // A sequence of full packed nvlists back to back; keep
                    // them as a list so vdev hierarchies survive.
                    let mut list = Vec::new();
                    for _ in 0..nelem {
                        let nested = Self::parse_at(&mut r)?;
                        list.push(nested);
                    }
                    NvValue::NvListArray(list)
                }
                other => {
                    // Unknown type: cannot know its width, and the stream
                    // is then untrusted.
                    return Err(ZfsError::Corrupt(format!(
                        "nvlist: unknown data type {other}"
                    )));
                }
            };
            out.pairs.push((name, value));
            if out.pairs.len() > 512 {
                return Err(ZfsError::Corrupt("nvlist: too many pairs".into()));
            }
        }
        Ok(out)
    }

    /// Parse a nested nvlist starting at the reader's current position.
    fn parse_at(r: &mut Reader<'_>) -> ZfsResult<NvList> {
        Self::parse_from(r)
    }

    fn parse_from(r: &mut Reader<'_>) -> ZfsResult<NvList> {
        // header
        r.u32()
            .ok_or_else(|| ZfsError::Corrupt("nvlist: nested hdr0".into()))?;
        let version = r
            .u32()
            .ok_or_else(|| ZfsError::Corrupt("nvlist: nested hdr1".into()))?;
        if version > 1 {
            return Err(ZfsError::Corrupt(format!(
                "nvlist: unsupported nested version {version}"
            )));
        }
        r.u32()
            .ok_or_else(|| ZfsError::Corrupt("nvlist: nested hdr2".into()))?;
        let mut out = NvList::default();
        loop {
            let _enc = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: nested pair hdr0".into()))?;
            let dec = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: nested pair hdr1".into()))?;
            if dec == 0 {
                break;
            }
            let name = r
                .xdr_string()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: nested name".into()))?;
            let type_id = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: nested type".into()))?;
            let nelem = r
                .u32()
                .ok_or_else(|| ZfsError::Corrupt("nvlist: nested nelem".into()))?;
            let value = match type_id {
                types::BOOLEAN | types::BOOLEAN_VALUE => {
                    if type_id == types::BOOLEAN_VALUE {
                        r.skip(4);
                    }
                    NvValue::Boolean
                }
                types::INT64 | types::UINT64 => NvValue::U64(r.u64().unwrap_or(0)),
                types::STRING => {
                    let s = r
                        .xdr_string()
                        .ok_or_else(|| ZfsError::Corrupt("nvlist: nested string".into()))?;
                    NvValue::String(s)
                }
                types::NVLIST => NvValue::NvList(Self::parse_from(r)?),
                types::NVLIST_ARRAY => {
                    let mut list = Vec::new();
                    for _ in 0..nelem {
                        list.push(Self::parse_from(r)?);
                    }
                    NvValue::NvListArray(list)
                }
                t => {
                    r.skip(4 * nelem.max(1) as usize);
                    NvValue::Other { type_id: t, nelem }
                }
            };
            out.pairs.push((name, value));
            if out.pairs.len() > 512 {
                return Err(ZfsError::Corrupt("nvlist: nested too many".into()));
            }
        }
        Ok(out)
    }

    pub fn get(&self, name: &str) -> Option<&NvValue> {
        self.pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    pub fn get_u64(&self, name: &str) -> Option<u64> {
        match self.get(name) {
            Some(NvValue::U64(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn get_str(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(NvValue::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn get_nvlist(&self, name: &str) -> Option<&NvList> {
        match self.get(name) {
            Some(NvValue::NvList(l)) => Some(l),
            _ => None,
        }
    }

    pub fn get_nvlist_array(&self, name: &str) -> Option<&[NvList]> {
        match self.get(name) {
            Some(NvValue::NvListArray(l)) => Some(l),
            Some(NvValue::NvList(l)) => Some(std::slice::from_ref(l)),
            _ => None,
        }
    }

    pub fn pairs(&self) -> &[(String, NvValue)] {
        &self.pairs
    }
}
