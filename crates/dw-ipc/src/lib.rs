//! # dw-ipc
//!
//! Typed IPC contract between the Rust core and the React/TS UI.
//!
//! All types here derive `serde::Serialize`/`Deserialize` AND `specta::Type` so
//! that `tauri-specta` can emit matching TypeScript definitions into the UI
//! package. This is the single source of truth for the wire format -- never
//! hand-write TS types.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod commands;
pub mod events;
pub mod types;

pub use commands::*;
pub use events::*;
pub use types::*;
