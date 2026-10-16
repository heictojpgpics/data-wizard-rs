//! Windows disk and volume enumeration via `DeviceIoControl`.
//!
//! The layer that answers "what storage exists on this machine" with
//! authoritative data straight from the storage stack:
//!
//! * **Physical drives** (`\\.\PhysicalDrive0..N`) — size from
//!   `IOCTL_DISK_GET_LENGTH_INFO` (the only reliable size source for
//!   device handles; `GetFileSizeEx` returns nothing useful for them),
//!   sector size from `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`, and
//!   model / serial / bus type / removable-media flag from
//!   `IOCTL_STORAGE_QUERY_PROPERTY`.
//! * **Volumes** (drive letters) — label + filesystem name from
//!   `GetVolumeInformationW`, total / free bytes from
//!   `GetDiskFreeSpaceExW`, kind from `GetDriveTypeW`, and the
//!   owning physical disk + extent from
//!   `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` — the mapping the UI uses
//!   to nest volumes under their parent drive's partitions.
//!
//! Handles are opened with `GENERIC_READ` first (needed when the
//! process is elevated, which the app manifest guarantees) and retried
//! with a query-only access mask of `0` so enumeration still reports
//! correct sizes, models and bus types when raw content reads are not
//! permitted.
//!
//! # Safety
//!
//! `unsafe` is used for Win32 FFI (handle lifetimes, `DeviceIoControl`
//! buffers). All IOCTL payload structs are parsed from raw little-endian
//! bytes with explicit bounds checks rather than transmuted.

#![cfg(windows)]
#![allow(unsafe_code)]

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW,
    GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::PCWSTR;

// ---------------------------------------------------------------------------
// IOCTL codes (CTL_CODE values from the WDK, frozen ABI)
// ---------------------------------------------------------------------------

/// `CTL_CODE(IOCTL_DISK_BASE, 0x0017, METHOD_BUFFERED, FILE_ANY_ACCESS)`
const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007_405C;
/// `CTL_CODE(IOCTL_DISK_BASE, 0x0028, METHOD_BUFFERED, FILE_ANY_ACCESS)`
const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_40A0;
/// `CTL_CODE(IOCTL_STORAGE_BASE, 0x0500, METHOD_BUFFERED, FILE_ANY_ACCESS)`
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
/// `CTL_CODE(IOCTL_VOLUME_BASE, 0x0000, METHOD_BUFFERED, FILE_ANY_ACCESS)`
const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;

/// `GENERIC_READ` as a raw access mask (dwDesiredAccess is DWORD in the
/// raw API; the typed `GENERIC_READ` constant is `u32` already).
const GENERIC_READ_RAW: u32 = 0x8000_0000;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// One physical drive as reported by the storage stack.
#[derive(Debug, Clone)]
pub struct PhysicalDriveInfo {
    /// Device path, e.g. `\\.\PhysicalDrive0`.
    pub path: String,
    /// Drive number parsed from the path (0, 1, ...).
    pub index: u32,
    /// Exact byte length from `IOCTL_DISK_GET_LENGTH_INFO`.
    pub size_bytes: u64,
    /// Bytes per sector from `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`
    /// (512 or 4096 on Advanced Format drives). Defaults to 512 when
    /// the geometry query is refused.
    pub bytes_per_sector: u32,
    /// Vendor + product string, e.g. `Samsung SSD 970 EVO 1TB`.
    pub model: Option<String>,
    /// Serial number, when the device reports one.
    pub serial: Option<String>,
    /// Bus the device is attached to.
    pub bus: StorageBus,
    /// The descriptor's `RemovableMedia` flag.
    pub removable_media: bool,
}

/// Bus classification derived from `STORAGE_DEVICE_DESCRIPTOR::BusType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBus {
    Unknown,
    Ata,
    Sata,
    Nvme,
    Usb,
    Sd,
    Mmc,
    Virtual,
    IScsi,
    FibreChannel,
    Raid,
    Sas,
    FireWire,
}

/// One mounted volume (drive letter).
#[derive(Debug, Clone)]
pub struct VolumeEntry {
    /// Drive letter, e.g. `C`.
    pub letter: String,
    /// `DRIVE_*` classification.
    pub kind: VolumeKind,
    /// Filesystem volume label, when present and readable.
    pub label: Option<String>,
    /// Filesystem name, e.g. `NTFS`, `exFAT`, `FAT32`.
    pub fs_name: Option<String>,
    /// Total capacity in bytes (`GetDiskFreeSpaceExW`).
    pub total_bytes: Option<u64>,
    /// Free bytes (`GetDiskFreeSpaceExW`).
    pub free_bytes: Option<u64>,
    /// Owning physical drive number from
    /// `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` (first extent).
    pub disk_number: Option<u32>,
    /// First extent's starting offset on the owning physical drive.
    pub extent_start: Option<u64>,
    /// First extent's length in bytes.
    pub extent_len: Option<u64>,
}

/// `GetDriveTypeW` classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKind {
    Unknown,
    NoRoot,
    Removable,
    Fixed,
    Remote,
    CdRom,
    RamDisk,
}

/// `GetDriveTypeW` return values (winbase.h; not exposed by the
/// windows crate's FileSystem module in 0.61).
const DRIVE_UNKNOWN: u32 = 0;
const DRIVE_NO_ROOT_DIR: u32 = 1;
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

// ---------------------------------------------------------------------------
// Physical drives
// ---------------------------------------------------------------------------

/// Enumerate `\\.\PhysicalDrive0..63`, skipping indices that do not
/// resolve to an openable device.
///
/// Every reported drive has an authoritative byte size; drives whose
/// size cannot be queried are skipped (a size-less drive entry is the
/// original "disk space is broken" bug this module exists to prevent).
pub fn enumerate_physical_drives() -> Vec<PhysicalDriveInfo> {
    let mut out = Vec::new();
    for n in 0..64u32 {
        let path = format!("\\\\.\\PhysicalDrive{n}");
        let Some(info) = query_physical_drive(&path, n) else {
            continue;
        };
        out.push(info);
    }
    out
}

fn query_physical_drive(path: &str, index: u32) -> Option<PhysicalDriveInfo> {
    // Try read access first (elevated context), then query-only.
    let handle = match open_device(Path::new(path), GENERIC_READ_RAW) {
        Some(h) => h,
        None => open_device(Path::new(path), 0)?,
    };

    let size = match disk_length(handle.0) {
        Some(s) if s > 0 => s,
        _ => return None, // no authoritative size -> not a real usable disk
    };
    let (bytes_per_sector, _disk_size_geom) = disk_geometry(handle.0).unwrap_or((512, size));
    let descriptor = storage_device_descriptor(handle.0);

    let (model, serial, bus, removable_media) = match &descriptor {
        Some(d) => (
            d.model.clone(),
            d.serial.clone(),
            d.bus,
            d.removable_media
                || matches!(d.bus, StorageBus::Usb | StorageBus::Sd | StorageBus::Mmc),
        ),
        None => (None, None, StorageBus::Unknown, false),
    };

    Some(PhysicalDriveInfo {
        path: path.to_string(),
        index,
        size_bytes: size,
        bytes_per_sector,
        model,
        serial,
        bus,
        removable_media,
    })
}

// ---------------------------------------------------------------------------
// Volumes
// ---------------------------------------------------------------------------

/// Enumerate all drive-letter volumes with label, filesystem, capacity
/// and physical-disk mapping.
///
/// Volumes whose handle cannot be opened at all (no media in a CD
/// drive, detached BitLocker) are still reported with kind `NoRoot` /
/// `Unknown` where the APIs refuse, so the UI can explain them instead
/// of silently hiding drive letters the user can see in Explorer.
pub fn enumerate_volumes() -> Vec<VolumeEntry> {
    let mut buf = [0u16; 512];
    // SAFETY: GetLogicalDriveStringsW receives a valid mutable buffer.
    let len = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) };
    if len == 0 || len as usize > buf.len() {
        return Vec::new();
    }
    let strings = String::from_utf16_lossy(&buf[..len as usize]);

    let mut out = Vec::new();
    for root in strings.split('\0') {
        // Each entry looks like "C:\".
        if root.len() < 3 || !root.ends_with('\\') {
            continue;
        }
        let letter = root[..1].to_string();
        if letter.is_empty() {
            continue;
        }

        // `root` is already the volume root form ("C:\").
        let root_w = to_wide(root);
        // SAFETY: valid NUL-terminated wide path.
        let ty = unsafe { GetDriveTypeW(PCWSTR(root_w.as_ptr())) };
        let kind = match ty {
            DRIVE_REMOVABLE => VolumeKind::Removable,
            DRIVE_FIXED => VolumeKind::Fixed,
            DRIVE_REMOTE => VolumeKind::Remote,
            DRIVE_CDROM => VolumeKind::CdRom,
            DRIVE_RAMDISK => VolumeKind::RamDisk,
            DRIVE_UNKNOWN => VolumeKind::Unknown,
            DRIVE_NO_ROOT_DIR => VolumeKind::NoRoot,
            _ => VolumeKind::Unknown,
        };

        let (label, fs_name) = volume_label_fs(root);
        let (total, free) = volume_space(root);
        let (disk_number, extent_start, extent_len) = volume_disk_extents(&letter);

        out.push(VolumeEntry {
            letter,
            kind,
            label,
            fs_name,
            total_bytes: total,
            free_bytes: free,
            disk_number,
            extent_start,
            extent_len,
        });
    }
    out
}

fn volume_label_fs(root: &str) -> (Option<String>, Option<String>) {
    let root_w = to_wide(root);
    let mut name_buf = [0u16; 261];
    let mut fs_buf = [0u16; 261];
    // SAFETY: valid NUL-terminated wide path; the slice buffers carry
    // their own lengths through the 0.61 signature.
    let ok = unsafe {
        GetVolumeInformationW(
            PCWSTR(root_w.as_ptr()),
            Some(&mut name_buf),
            None,
            None,
            None,
            Some(&mut fs_buf),
        )
    };
    if ok.is_err() {
        return (None, None);
    }
    let label = wide_to_string(&name_buf);
    let fs = wide_to_string(&fs_buf);
    (label, fs)
}

fn volume_space(root: &str) -> (Option<u64>, Option<u64>) {
    let root_w = to_wide(root);
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    // SAFETY: valid NUL-terminated wide path, optional out pointers.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(root_w.as_ptr()),
            None,
            Some(&mut total),
            Some(&mut free),
        )
    };
    if ok.is_err() {
        return (None, None);
    }
    (Some(total), Some(free))
}

/// `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` on `\\.\C:` — the volume ->
/// physical disk + offset mapping. Query-only access is sufficient.
fn volume_disk_extents(letter: &str) -> (Option<u32>, Option<u64>, Option<u64>) {
    let path = format!("\\\\.\\{letter}:");
    // Volume IOCTLs accept a 0 access mask; GENERIC_READ needs elevation.
    let Some(handle) = open_device(Path::new(&path), 0) else {
        return (None, None, None);
    };
    // VOLUME_DISK_EXTENTS: NumberOfDiskExtents (u32) + up to 4 extents
    // of 24 bytes each, starting at offset 8.
    let mut out = [0u8; 8 + 24 * 4];
    let mut returned: u32 = 0;
    // SAFETY: valid handle, sized out buffer.
    let ok = unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
            None,
            0,
            Some(out.as_mut_ptr().cast()),
            out.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() || returned < 8 || read_u32(&out, 0) == Some(0) {
        return (None, None, None);
    }
    let disk = read_u32(&out, 8);
    let start = read_u64(&out, 16);
    let len = read_u64(&out, 24);
    (disk, start, len)
}

// ---------------------------------------------------------------------------
// IOCTL helpers
// ---------------------------------------------------------------------------

/// `IOCTL_DISK_GET_LENGTH_INFO` — authoritative byte size for volume
/// and physical-drive handles.
fn disk_length(handle: HANDLE) -> Option<u64> {
    let mut out = [0u8; 8];
    let mut returned: u32 = 0;
    // SAFETY: valid handle, 8-byte out buffer for LARGE_INTEGER.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            None,
            0,
            Some(out.as_mut_ptr().cast()),
            out.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() || returned < 8 {
        return None;
    }
    read_u64(&out, 0)
}

/// `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` — real bytes-per-sector
/// (4096 on Advanced Format drives) plus a second size opinion.
fn disk_geometry(handle: HANDLE) -> Option<(u32, u64)> {
    // DISK_GEOMETRY_EX: Geometry (24) + DiskSize (8) + tail; 64 bytes
    // is comfortably above the minimum and a supported buffer size.
    let mut out = [0u8; 64];
    let mut returned: u32 = 0;
    // SAFETY: valid handle, sized out buffer.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
            None,
            0,
            Some(out.as_mut_ptr().cast()),
            out.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() || returned < 32 {
        return None;
    }
    // DISK_GEOMETRY: Cylinders i64 @0, MediaType u32 @8,
    // TracksPerCylinder u32 @12, SectorsPerTrack u32 @16,
    // BytesPerSector u32 @20. DiskSize i64 @24.
    let bps = read_u32(&out, 20)?;
    let size = read_u64(&out, 24)?;
    if bps == 0 {
        return None;
    }
    Some((bps, size))
}

/// Parsed `STORAGE_DEVICE_DESCRIPTOR` payload.
struct DeviceDescriptor {
    model: Option<String>,
    serial: Option<String>,
    bus: StorageBus,
    removable_media: bool,
}

/// `IOCTL_STORAGE_QUERY_PROPERTY` with `StorageDeviceProperty`.
fn storage_device_descriptor(handle: HANDLE) -> Option<DeviceDescriptor> {
    // STORAGE_PROPERTY_QUERY { PropertyId: 0 (StorageDeviceProperty),
    // QueryType: 0 (PropertyStandardQuery), AdditionalParameters[1] }.
    let query = [0u8; 12];
    // The descriptor plus its strings; 4 KiB covers real hardware
    // (descriptor header is 36 bytes, strings follow inline).
    let mut out = vec![0u8; 4096];
    let mut returned: u32 = 0;
    // SAFETY: valid handle, query buffer and sized out buffer.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(query.as_ptr().cast()),
            query.len() as u32,
            Some(out.as_mut_ptr().cast()),
            out.len() as u32,
            Some(&mut returned),
            None,
        )
    };
    if ok.is_err() || returned < 36 {
        return None;
    }

    // Offsets inside STORAGE_DEVICE_DESCRIPTOR; a value of 0 means
    // "not present". String offsets are relative to the struct start.
    let removable = out.get(10).copied().unwrap_or(0) != 0;
    let bus_raw = read_u32(&out, 28).unwrap_or(0);
    let bus = bus_from_storage_bus_type(bus_raw);
    let vendor = string_at(&out, read_u32(&out, 12).unwrap_or(0));
    let product = string_at(&out, read_u32(&out, 16).unwrap_or(0));
    let revision = string_at(&out, read_u32(&out, 20).unwrap_or(0));
    let serial = string_at(&out, read_u32(&out, 24).unwrap_or(0)).map(trim_serial);

    let model = compose_model(vendor, product, revision);

    Some(DeviceDescriptor {
        model,
        serial,
        bus,
        removable_media: removable,
    })
}

/// STORAGE_BUS_TYPE raw values (wdm.h / ntddstor.h).
fn bus_from_storage_bus_type(v: u32) -> StorageBus {
    match v {
        0 => StorageBus::Unknown,
        1..=3 => StorageBus::Ata,  // Scsi, Atapi, Ata
        4 => StorageBus::FireWire,     // 1394
        5 => StorageBus::FibreChannel, // SSA treated as FC family
        6 => StorageBus::FibreChannel,
        7 => StorageBus::Usb,
        8 => StorageBus::Raid,
        9 => StorageBus::IScsi,
        10 => StorageBus::Sas,
        11 => StorageBus::Sata,
        12 => StorageBus::Sd,
        13 => StorageBus::Mmc,
        14 | 15 => StorageBus::Virtual, // Virtual, FileBackedVirtual
        16 => StorageBus::Virtual,      // StorageSpaces
        17 => StorageBus::Nvme,
        18 => StorageBus::Virtual, // SCM / others -> virtual class
        _ => StorageBus::Unknown,
    }
}

/// Compose the human model string: `Vendor Product` when both exist,
/// else whichever exists. Hardware strings are single-quoted by the
/// storage stack (`'Samsung '`) — quotes and surrounding whitespace
/// are trimmed.
fn compose_model(
    vendor: Option<String>,
    product: Option<String>,
    revision: Option<String>,
) -> Option<String> {
    let clean = |s: &Option<String>| {
        s.as_deref()
            .map(|v| v.trim().trim_matches('\'').trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let vendor = clean(&vendor);
    let product = clean(&product);
    let revision = clean(&revision);

    let base = match (&vendor, &product) {
        (Some(v), Some(p)) => Some(format!("{v} {p}")),
        (None, Some(p)) => Some(p.clone()),
        (Some(v), None) => Some(v.clone()),
        (None, None) => None,
    };
    match (base, revision) {
        (Some(b), Some(r)) if !b.contains(&r) => Some(format!("{b} {r}")),
        (Some(b), _) => Some(b),
        (None, r) => r,
    }
}

/// Serials are frequently reported in SCSI hex-ASCII form
/// (`52583d4d4d4d4d...`) — trim to a displayable prefix.
fn trim_serial(s: String) -> String {
    let t = s.trim().trim_matches('\'').trim();
    // Hex-encoded serials pack two ASCII chars per byte; keep the raw
    // string as reported (tooling matches on it) but cap length.
    t.chars().take(40).collect()
}

fn string_at(buf: &[u8], offset: u32) -> Option<String> {
    if offset == 0 {
        return None;
    }
    let start = offset as usize;
    if start >= buf.len() {
        return None;
    }
    let slice = &buf[start..];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    let bytes = &slice[..end];
    let text = bytes.iter().map(|&b| b as char).collect::<String>();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
    let s = buf.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes(s.try_into().ok()?))
}

fn read_u64(buf: &[u8], off: usize) -> Option<u64> {
    let s = buf.get(off..off.checked_add(8)?)?;
    Some(u64::from_le_bytes(s.try_into().ok()?))
}

// ---------------------------------------------------------------------------
// Handle plumbing
// ---------------------------------------------------------------------------

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: handle is valid for the lifetime of this struct.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

fn open_device(path: &Path, access: u32) -> Option<OwnedHandle> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: NUL-terminated wide path; standard creation flags.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };
    match handle {
        Ok(h) if h != INVALID_HANDLE_VALUE && !h.is_invalid() => Some(OwnedHandle(h)),
        _ => None,
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_to_string(buf: &[u16]) -> Option<String> {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..end]);
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Whether the current process can perform raw disk reads — probed
/// directly by opening the first physical drive with `GENERIC_READ`.
///
/// This is the property the app actually depends on (elevated token or
/// not is an implementation detail); the embedded app manifest requests
/// `requireAdministrator`, so this holds in normal operation.
pub fn has_raw_disk_access() -> bool {
    for n in 0..4u32 {
        if open_device(
            Path::new(&format!("\\\\.\\PhysicalDrive{n}")),
            GENERIC_READ_RAW,
        )
        .is_some()
        {
            return true;
        }
    }
    false
}
