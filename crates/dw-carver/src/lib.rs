//! # dw-carver
//!
//! Raw file carving engine.
//!
//! Algorithms implemented (all clean-room, derived from published
//! digital-forensics literature):
//!
//!  1. **Header / footer carving** -- scan for a known header magic,
//!     then scan forward for the matching footer. Trim to the
//!     header..footer range, validate, emit. Classic Foremost/Scalpel
//!     approach.
//!
//!  2. **Bifragment gap carving** (Richard & Roussev, "Forensic File
//!     Carving", 2006) -- for files that have exactly two fragments
//!     separated by a small overwriting gap. We try every plausible
//!     split point within a configurable search window, validate
//!     against the file's structural checksum (JPEG EOI, ZIP EOCD,
//!     PNG IEND, etc.), and accept the first validation pass.
//!
//!  3. **Structured carving** -- for file types with rich internal
//!     structure (e.g. ZIP central directory, RAR archive end header,
//!     PDF xref table, ext4 superblock), we walk that structure to
//!     compute the actual end of the file rather than trusting the
//!     footer scan.
//!
//!  4. **Map carving** -- for fragmented files with many small
//!     fragments, we maintain a per-signature "fragment candidate"
//!     heap and search for an assignment that satisfies the file's
//!     internal ordering constraints. Expensive -- only invoked when
//!     bifragment carving fails AND the user opted into deep mode.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod bifragment;
pub mod error;
pub mod fragment;
pub mod header_footer;
pub mod plan;
pub mod structured;

pub use error::{CarverError, CarverResult};
pub use plan::{CarveParams, CarvePlan, CarveStrategy};
