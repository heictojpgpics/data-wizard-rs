//! Standalone NTFS scanner — opens a file-backed image, runs the NTFS
//! plugin, and prints recovered entries.
//!
//! Usage:
//! ```sh
//! cargo run --example scan_image -- /path/to/image.img
//! ```
//!
//! This is used by the CI verification workflow to test recovery against
//! a real NTFS VHD. It's also useful for manual debugging.

use std::path::PathBuf;
use std::sync::Arc;

use dw_core::{FsPlugin, ProbeOutcome, ProgressEvent, ProgressSink, ResultsStore, ScanMode};
use dw_disk::FileBackedSource;
use dw_ntfs::NtfsPlugin;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info,dw_ntfs=debug")
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: scan_image <path-to-image> [--deep]");
        std::process::exit(1);
    }
    let path = PathBuf::from(&args[1]);
    let deep = args.iter().any(|a| a == "--deep");
    let mode = if deep {
        ScanMode::Deep
    } else {
        ScanMode::Quick
    };

    tracing::info!(path = %path.display(), ?mode, "Opening image");

    let source = Arc::new(FileBackedSource::open(&path)?);
    let plugin = NtfsPlugin;

    // Probe
    match plugin.probe(source.as_ref()).await? {
        ProbeOutcome::Yes(probe) => {
            tracing::info!(
                fs_kind = %probe.fs_kind,
                volume_serial = ?probe.volume_serial,
                total_bytes = probe.total_bytes,
                cluster_size = probe.cluster_size,
                "Probe succeeded"
            );
        }
        ProbeOutcome::No => {
            tracing::error!("Not an NTFS volume");
            std::process::exit(2);
        }
        ProbeOutcome::Corrupt(msg) => {
            tracing::error!(reason = %msg, "NTFS volume is corrupt");
            std::process::exit(3);
        }
    }

    // Scan
    let results = ResultsStore::new(1_000_000, None);
    let progress = Arc::new(CliProgressSink);
    plugin.scan(source, mode, results.clone(), progress).await?;

    // Print results
    let snapshot = results.snapshot();
    println!("\n=== Recovery Results ===");
    println!("Total entries: {}", snapshot.len());
    println!("Spilled: {}", results.spilled_count());

    let deleted_count = snapshot.iter().filter(|e| e.was_deleted).count();
    let intact_count = snapshot.iter().filter(|e| !e.was_deleted).count();
    println!("Live: {intact_count}, Deleted: {deleted_count}");

    println!("\n--- First 50 entries ---");
    for entry in snapshot.iter().take(50) {
        let status = if entry.was_deleted { "DEL" } else { "   " };
        println!(
            "[{status}] {:>10}  {}/{}",
            format_size(entry.size),
            entry.parent_path,
            entry.name
        );
    }

    if snapshot.len() > 50 {
        println!("... and {} more", snapshot.len() - 50);
    }

    Ok(())
}

struct CliProgressSink;

#[async_trait::async_trait]
impl ProgressSink for CliProgressSink {
    async fn emit(&self, event: ProgressEvent) {
        let pct = (event.current * 100)
            .checked_div(event.total)
            .map(|p| p.min(100))
            .unwrap_or(0);
        eprintln!(
            "[{:?}] {:3}%  {}  ({}/{})",
            event.phase, pct, event.message, event.current, event.total
        );
    }

    async fn fail(&self, message: String) {
        eprintln!("[FAILED] {message}");
    }

    async fn done(&self) {
        eprintln!("[DONE]");
    }
}

fn format_size(n: u64) -> String {
    if n < 1024 {
        format!("{}B", n)
    } else if n < 1024 * 1024 {
        format!("{}K", n / 1024)
    } else if n < 1024 * 1024 * 1024 {
        format!("{}M", n / (1024 * 1024))
    } else {
        format!("{}G", n / (1024 * 1024 * 1024))
    }
}
