//! FAT-family timestamp decoding.
//!
//! Moved to `dw_core::fat_timestamp` so both the FAT and exFAT engines
//! share one strict implementation (no cross-crate sibling dependency).
//! Re-exported here so existing `crate::timestamp` import paths keep
//! working.

pub use dw_core::fat_timestamp::{decode_fat_access_date, decode_fat_datetime};
