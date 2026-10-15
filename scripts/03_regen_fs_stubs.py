#!/usr/bin/env python3
"""Regenerate per-FS error.rs and plugin.rs with proper names."""
from pathlib import Path

ROOT = Path("/home/z/my-project/crates")

# (dir, error_name, result_name, plugin_name, id_str, display_name)
FS_CRATES = [
    ("dw-ntfs",   "NtfsError",   "NtfsResult",   "NtfsPlugin",   "ntfs",   "NTFS 3.x parser"),
    ("dw-fat",    "FatError",    "FatResult",    "FatPlugin",    "fat",    "FAT12/16/32 parser"),
    ("dw-exfat",  "ExfatError",  "ExfatResult",  "ExfatPlugin",  "exfat",  "exFAT parser"),
    ("dw-ext",    "ExtError",    "ExtResult",    "ExtPlugin",    "ext",    "ext2/3/4 parser"),
    ("dw-hfs",    "HfsError",    "HfsResult",    "HfsPlugin",    "hfs",    "HFS+ parser"),
    ("dw-btrfs",  "BtrfsError",  "BtrfsResult",  "BtrfsPlugin",  "btrfs",  "Btrfs parser"),
    ("dw-refs",   "RefsError",   "RefsResult",   "RefsPlugin",   "refs",   "ReFS parser"),
    ("dw-zfs",    "ZfsError",    "ZfsResult",    "ZfsPlugin",    "zfs",    "ZFS parser"),
]

ERROR_TMPL = '''use thiserror::Error;

pub type {result_name}<T> = Result<T, {error_name}>;

#[derive(Debug, Error)]
pub enum {error_name} {{
    #[error("I/O error: {{0}}")]
    Io(#[from] std::io::Error),
    #[error("disk error: {{0}}")]
    Disk(#[from] dw_disk::DiskError),
    #[error("corrupt on-disk structure: {{0}}")]
    Corrupt(String),
    #[error("out of range: offset={{offset}}, len={{len}}")]
    OutOfRange {{ offset: u64, len: u64 }},
    #[error("not yet implemented: {{0}}")]
    Todo(&'static str),
}}
'''

PLUGIN_TMPL = '''//! FsPlugin impl stub. To be filled in with real parser logic.

use std::sync::Arc;
use async_trait::async_trait;
use dw_core::{{FsPlugin, ProbeOutcome, ScanMode, ProgressSink, ResultsStore, CoreResult}};
use dw_disk::DiskSource;

pub struct {plugin_name};

impl Default for {plugin_name} {{
    fn default() -> Self {{ Self }}
}}

#[async_trait]
impl FsPlugin for {plugin_name} {{
    fn id(&self) -> &'static str {{ "{id_str}" }}
    fn name(&self) -> &'static str {{ "{display_name} (stub)" }}
    async fn probe(&self, _source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {{
        // TODO: real probe logic
        Ok(ProbeOutcome::No)
    }}
    async fn scan(
        &self,
        _source: Arc<dyn DiskSource>,
        _mode: ScanMode,
        _results: Arc<ResultsStore>,
        _progress: Arc<dyn ProgressSink>,
    ) -> CoreResult<()> {{
        // TODO: real scan logic
        Ok(())
    }}
}}
'''

for dirname, err, res, plug, id_str, display in FS_CRATES:
    d = ROOT / dirname
    (d / "src" / "error.rs").write_text(ERROR_TMPL.format(error_name=err, result_name=res))
    (d / "src" / "plugin.rs").write_text(PLUGIN_TMPL.format(
        plugin_name=plug, id_str=id_str, display_name=display))
    print(f"  wrote {dirname}/src/error.rs and plugin.rs ({plug})")

print("done")
