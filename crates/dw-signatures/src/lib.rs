//! # dw-signatures
//!
//! File-signature database used by [`dw-carver`] to classify carved chunks.
//!
//! ## IMPORTANT -- data provenance
//!
//! The signature database is rebuilt **entirely from public sources**. We do
//! NOT extract, copy, or redistribute any proprietary signature table from
//! any commercial product. Public sources we use:
//!
//!  - Gary Kessler's GCF "File Signatures Table"
//!    <https://www.garykessler.net/software/filemountains.txt>
//!  - NIST DFXML "Digital Forensics XML" file-signature set
//!  - The PRONOM registry (UK National Archives)
//!  - TrID's database (free subset)
//!  - `file` (libmagic) magic rules
//!  - Wikipedia "List of file signatures"
//!
//! ## On-disk format (v1)
//!
//! Compiled at build time into a single `signatures.bin` file consumed via
//! `memmap2`. The format is:
//!
//! ```text
//!   Header (32 bytes):
//!     magic     : 4  "DWSG"            // "Data-Wizard SiGnatures"
//!     version   : 2  u16 LE = 1
//!     count     : 4  u32 LE
//!     flags     : 4  u32 LE (bit 0 = has-tail)
//!     reserved  : 18 zero
//!   Records (count * N, variable length):
//!     ext_len   : 1  u8
//!     ext       : ext_len bytes ASCII (no NUL)
//!     mime_len  : 1  u8
//!     mime      : mime_len bytes ASCII
//!     hdr_off   : 4  u32 LE   offset where header magic is expected
//!     hdr_len   : 2  u16 LE   length of header magic in bytes
//!     hdr_magic : hdr_len bytes
//!     tail_off  : 4  u32 LE   offset from END (tail-relative) where tail magic is expected (0 = no tail)
//!     tail_len  : 2  u16 LE
//!     tail_magic: tail_len bytes (omitted if tail_off = 0)
//!     min_size  : 8  u64 LE
//!     max_size  : 8  u64 LE (0 = unlimited)
//!     weight    : 2  u16 LE  0..=1000 (confidence in 0/1000)
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod db;
pub mod error;
pub mod loader;
pub mod matcher;
pub mod spec;

pub use db::{SignatureDb, SignatureId};
pub use error::{SignaturesError, SignaturesResult};
pub use loader::build_db_from_sources;
pub use matcher::{MatchResult, Matcher};
pub use spec::{MagicByte, Signature, SignatureSpec};
