//! Generic volume scanner: probe all engines, run the matching one, and
//! print a results summary. Used by the CI verification workflows
//! (data-wizard-ci) against real filesystems created on Windows runners,
//! and as a manual diagnostic tool.
//!
//! Usage:
//!   scan_volume <image-or-device> [--deep] [--partitions] [--partition N]
//!
//! --partitions lists the partition table + probed fs per partition.
//! --partition N scans only partition N (PartitionView, no copies).
//! Without --partition, the whole source is scanned as one volume.

use std::sync::Arc;

use dw_core::plugin::{FsPlugin, ProbeOutcome, ScanMode};
use dw_core::progress::{ProgressEvent, ProgressSink};
use dw_core::results::{RecoveryStatus, ResultsStore};
use dw_disk::{DiskSource, FileBackedSource, PartitionView};

fn all_plugins() -> Vec<Arc<dyn FsPlugin>> {
    vec![
        Arc::new(dw_ntfs::NtfsPlugin),
        Arc::new(dw_fat::FatPlugin),
        Arc::new(dw_exfat::ExfatPlugin),
        Arc::new(dw_ext::ExtPlugin),
        Arc::new(dw_hfs::HfsPlugin),
        Arc::new(dw_btrfs::BtrfsPlugin),
    ]
}

struct PrintSink;

#[async_trait::async_trait]
impl ProgressSink for PrintSink {
    async fn emit(&self, ev: ProgressEvent) {
        eprintln!("[progress] {:?} {}", ev.phase, ev.message);
    }
    async fn fail(&self, msg: String) {
        eprintln!("[failed] {msg}");
    }
    async fn done(&self) {
        eprintln!("[done]");
    }
}

#[tokio::main]
async fn main() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: scan_volume <source> [--deep] [--partitions] [--partition N]");
        std::process::exit(2);
    }
    let source_path = args[0].clone();
    let deep = args.iter().any(|a| a == "--deep");
    let list_parts = args.iter().any(|a| a == "--partitions");
    let partition: Option<u32> = args
        .iter()
        .position(|a| a == "--partition")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok());

    let file_source = match FileBackedSource::open(std::path::Path::new(&source_path)) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("error: open {source_path}: {e}");
            std::process::exit(1);
        }
    };

    // ---- Partition listing ----
    if list_parts {
        let table = match dw_disk::discover_partitions(file_source.as_read_at().as_ref()) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("partition discovery error: {e}");
                std::process::exit(1);
            }
        };
        match &table {
            dw_disk::DiscoveredTable::Gpt { entries, .. } => {
                println!("table: gpt, {} partitions", entries.len());
                for e in entries {
                    let fs =
                        probe_fs_partition(&file_source.as_read_at(), e.offset_bytes, e.size_bytes)
                            .await;
                    println!(
                        "  #{} @{:<10} size={:<12} type={:<22} label={:?} fs={:?}",
                        e.index, e.offset_bytes, e.size_bytes, e.type_name, e.label, fs
                    );
                }
            }
            dw_disk::DiscoveredTable::Mbr { entries } => {
                println!("table: mbr, {} partitions", entries.len());
                for e in entries {
                    let fs =
                        probe_fs_partition(&file_source.as_read_at(), e.offset_bytes, e.size_bytes)
                            .await;
                    println!(
                        "  #{} @{:<10} size={:<12} scheme={:<6} type={:<22} fs={:?}",
                        e.index, e.offset_bytes, e.size_bytes, e.scheme, e.type_name, fs
                    );
                }
            }
            dw_disk::DiscoveredTable::None => println!("table: none (raw volume)"),
        }
        return;
    }

    // ---- Source selection (whole volume or one partition) ----
    let source: Arc<dyn DiskSource> = match partition {
        Some(index) => {
            let table = match dw_disk::discover_partitions(file_source.as_read_at().as_ref()) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("partition discovery error: {e}");
                    std::process::exit(1);
                }
            };
            let entries = match &table {
                dw_disk::DiscoveredTable::Gpt { entries, .. } => entries,
                dw_disk::DiscoveredTable::Mbr { entries } => entries,
                dw_disk::DiscoveredTable::None => {
                    eprintln!("no partition table; cannot select partition {index}");
                    std::process::exit(1);
                }
            };
            let entry = entries
                .iter()
                .find(|e| e.index == index)
                .unwrap_or_else(|| {
                    eprintln!("partition {index} not found ({} present)", entries.len());
                    std::process::exit(1);
                });
            Arc::new(
                PartitionView::new(
                    file_source.as_read_at(),
                    entry.offset_bytes,
                    entry.size_bytes,
                )
                .expect("partition view"),
            )
        }
        None => file_source,
    };

    // ---- Probe ----
    let mode = if deep {
        ScanMode::Deep
    } else {
        ScanMode::Quick
    };
    println!(
        "source: {source_path} ({} bytes)",
        source.as_read_at().size()
    );
    println!("mode:   {:?}", mode);
    if let Some(fs) = probe_fs(source.as_ref()).await {
        println!("fs:     {fs}");
    } else {
        println!("fs:     unknown (no plugin claimed it)");
    }

    // ---- Scan ----
    let results = ResultsStore::new(1_000_000, None);
    let progress: Arc<dyn ProgressSink> = Arc::new(PrintSink);
    let plugins = all_plugins();
    let matched = probe_plugin(&plugins, source.as_ref()).await;
    let Some(plugin) = matched else {
        eprintln!(
            "no plugin claimed the source; nothing to scan (try --deep for raw carving via the app)"
        );
        return;
    };
    println!("engine: {}", plugin.name());
    // Ctrl-C wires straight into the cooperative cancel flag.
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let cancel = std::sync::Arc::clone(&cancel);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                eprintln!("cancelling...");
            }
        });
    }
    plugin
        .scan(source, mode, results.clone(), progress, cancel)
        .await
        .expect("scan");

    // ---- Summary ----
    let snapshot = results.snapshot();
    let intact = snapshot
        .iter()
        .filter(|e| e.status == RecoveryStatus::Intact)
        .count();
    let partial = snapshot
        .iter()
        .filter(|e| e.status == RecoveryStatus::PartiallyRecovered)
        .count();
    let validated = snapshot
        .iter()
        .filter(|e| e.status == RecoveryStatus::CarvedValidated)
        .count();
    let deleted = snapshot.iter().filter(|e| e.was_deleted).count();
    let ts = snapshot.iter().filter(|e| e.mtime.is_some()).count();
    let with_extents = snapshot.iter().filter(|e| !e.extents.is_empty()).count();
    println!();
    println!("Total entries: {}", snapshot.len());
    println!("  intact: {intact}, partial: {partial}, carved-validated: {validated}");
    println!("  deleted: {deleted}, with mtime: {ts}, with extents: {with_extents}");
    for e in snapshot.iter().take(10) {
        println!(
            "  {} {} size={} extents={} mtime={}",
            e.parent_path,
            e.name,
            e.size,
            e.extents.len(),
            e.mtime.is_some()
        );
    }
    if snapshot.len() > 10 {
        println!("  ... and {} more", snapshot.len() - 10);
    }
}

async fn probe_fs(view: &dyn DiskSource) -> Option<String> {
    for plugin in all_plugins() {
        if let Ok(ProbeOutcome::Yes(probe)) = plugin.probe(view).await {
            return Some(probe.fs_kind);
        }
    }
    None
}

async fn probe_fs_partition(
    read: &std::sync::Arc<dyn dw_disk::ReadAt>,
    off: u64,
    size: u64,
) -> Option<String> {
    let view = PartitionView::new(std::sync::Arc::clone(read), off, size).ok()?;
    probe_fs(&view).await
}

async fn probe_plugin(
    plugins: &[Arc<dyn FsPlugin>],
    source: &dyn DiskSource,
) -> Option<Arc<dyn FsPlugin>> {
    for plugin in plugins {
        if let Ok(ProbeOutcome::Yes(_)) = plugin.probe(source).await {
            return Some(Arc::clone(plugin));
        }
    }
    None
}
