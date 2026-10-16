//! `--diagnostics`: a headless self-check of the storage stack.
//!
//! Prints a JSON report of everything the app needs to function —
//! physical drives (sizes, models, bus types, partitions), volumes
//! (labels, filesystems, free space), and whether raw disk reads are
//! permitted — to stdout and exits without starting the UI.
//!
//! Purpose-built for two audiences:
//!
//! * **CI**: the installer smoke test runs `dw-tauri.exe --diagnostics`
//!   and asserts every disk has a nonzero size and at least one disk
//!   is present with raw access. The original release-shipping bug
//!   (sizes from `GetFileSizeEx` on device handles — zeroes and
//!   silently missing drives) is exactly the class of regression this
//!   catches at build time instead of on user machines.
//! * **Support**: users can run it themselves and attach the output;
//!   no console window pops up (output goes to the redirected handle
//!   only).
//!
//! Exit codes: `0` healthy, `1` storage stack unusable (no disks, or
//! every disk has a zero/garbage size), `2` raw reads unavailable
//! (elevation lost — the manifest should prevent this).

use serde::Serialize;

#[derive(Debug, Serialize)]
struct DiagnosticsReport {
    /// Product version from Cargo.
    version: String,
    /// OS string (Windows build family).
    os: String,
    /// Whether raw physical-drive reads are permitted.
    raw_disk_access: bool,
    /// Physical drives + unmapped volumes, exactly what `list_disks`
    /// reports to the UI.
    disks: Vec<DiskReport>,
    /// Count of mounted drive-letter volumes seen during enumeration.
    volumes: usize,
    /// Non-fatal anomalies worth surfacing (empty when healthy).
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DiskReport {
    id: String,
    display_name: String,
    size_bytes: u64,
    free_bytes: Option<u64>,
    model: Option<String>,
    bus_type: String,
    is_removable: bool,
    is_system: bool,
    partitions: Vec<PartitionReport>,
}

#[derive(Debug, Serialize)]
struct PartitionReport {
    index: u32,
    drive_letter: Option<String>,
    label: Option<String>,
    fs_kind: Option<String>,
    size_bytes: u64,
    free_bytes: Option<u64>,
}

/// Entry point: build the report, print JSON, return the exit code.
pub fn run() -> i32 {
    let report = build_report();
    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            use std::io::Write;
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            // Best-effort: when no handle is attached (double-clicked
            // from Explorer) the write is dropped silently rather than
            // panicking.
            let _ = writeln!(lock, "{json}");
            let _ = lock.flush();
        }
        Err(e) => {
            eprintln!("diagnostics serialization failed: {e}");
            return 1;
        }
    }

    // Verdict.
    if report.disks.is_empty() {
        eprintln!("diagnostics: no disks enumerated");
        return 1;
    }
    if report.disks.iter().all(|d| d.size_bytes == 0) {
        eprintln!("diagnostics: every disk reports a zero size");
        return 1;
    }
    if !report.raw_disk_access {
        eprintln!("diagnostics: raw disk reads unavailable (elevation required)");
        return 2;
    }
    0
}

fn build_report() -> DiagnosticsReport {
    let disks_dto = super::commands::enumerate_disks_public();
    let mut warnings = Vec::new();

    let disks: Vec<DiskReport> = disks_dto
        .iter()
        .map(|d| {
            if d.size_bytes == 0 {
                warnings.push(format!("disk {} reports size 0", d.id));
            }
            DiskReport {
                id: d.id.clone(),
                display_name: d.display_name.clone(),
                size_bytes: d.size_bytes,
                free_bytes: d.free_bytes,
                model: d.model.clone(),
                bus_type: format!("{:?}", d.bus_type),
                is_removable: d.is_removable,
                is_system: d.is_system,
                partitions: d
                    .partitions
                    .iter()
                    .map(|p| PartitionReport {
                        index: p.index,
                        drive_letter: p.drive_letter.clone(),
                        label: p.label.clone(),
                        fs_kind: p.fs_kind.clone(),
                        size_bytes: p.size_bytes,
                        free_bytes: p.free_bytes,
                    })
                    .collect(),
            }
        })
        .collect();

    #[cfg(windows)]
    let (raw_access, volume_count) = (
        dw_disk::has_raw_disk_access(),
        dw_disk::enumerate_volumes().len(),
    );
    #[cfg(not(windows))]
    let (raw_access, volume_count) = (false, 0);

    DiagnosticsReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        raw_disk_access: raw_access,
        disks,
        volumes: volume_count,
        warnings,
    }
}
