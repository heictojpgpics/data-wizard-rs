//! Windows raw-disk access via `CreateFileW` + `SetFilePointerEx` + `ReadFile`.
//!
//! Only compiled on Windows. The rest of the codebase talks to disks
//! through the [`DiskSource`](crate::DiskSource) trait, so this module is
//! the only place that has to deal with Windows HANDLEs.
//!
//! # Safety
//!
//! This module uses `unsafe` for Win32 FFI. The HANDLE is wrapped in a
//! struct that implements `Send` + `Sync` manually (Win32 HANDLEs are
//! thread-safe for read operations).

#![cfg(windows)]
#![allow(unsafe_code)]

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::Mutex;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING, ReadFile, SetFilePointerEx,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::PCWSTR;

use crate::source::{DiskSource, ReadAt};
use crate::{DiskError, DiskGeometry, DiskResult};

/// Wrapper around a Win32 `HANDLE` that is `Send + Sync`.
struct SendHandle(HANDLE);

unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

pub struct WindowsRawSource {
    handle: Arc<Mutex<SendHandle>>,
    size: u64,
    geometry: DiskGeometry,
}

impl WindowsRawSource {
    /// Open a Windows device path such as `\\.\PhysicalDrive0` or `\\.\C:`.
    pub fn open(path: &Path) -> DiskResult<Self> {
        let path_wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: CreateFileW is FFI; we pass a valid NUL-terminated wide
        // string and standard flags.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path_wide.as_ptr()),
                0x8000_0000, // GENERIC_READ
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };
        let handle = match handle {
            Ok(h) if h != INVALID_HANDLE_VALUE && !h.is_invalid() => h,
            _ => {
                return Err(DiskError::Windows(format!(
                    "CreateFileW failed for {path:?}"
                )));
            }
        };
        let size = query_size(handle)?;
        let geometry = DiskGeometry {
            total_bytes: size,
            bytes_per_sector: 512,
            sectors_per_track: 0,
            heads: 0,
            cylinders: 0,
        };
        Ok(Self {
            handle: Arc::new(Mutex::new(SendHandle(handle))),
            size,
            geometry,
        })
    }
}

impl Drop for SendHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

fn query_size(handle: HANDLE) -> DiskResult<u64> {
    // `GetFileSizeEx` does not work for device handles (physical drives,
    // raw volumes): the file systems do not answer FileStandardInformation
    // for them, so it fails or reports garbage — the original "disk space
    // is broken" bug. The authoritative size for a device handle is
    // IOCTL_DISK_GET_LENGTH_INFO.
    const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007_405C;
    let mut out = [0u8; 8];
    let mut returned: u32 = 0;
    // SAFETY: valid HANDLE, an 8-byte out buffer for LARGE_INTEGER.
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
    if let Ok(()) = ok
        && returned >= 8
        && let Ok(b) = out[..8].try_into()
    {
        let len = i64::from_le_bytes(b);
        if len > 0 {
            return Ok(len as u64);
        }
    }
    Err(DiskError::Windows(
        "IOCTL_DISK_GET_LENGTH_INFO failed for device handle".into(),
    ))
}

impl ReadAt for WindowsRawSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        let h = self.handle.lock();
        if h.0.is_invalid() {
            return Err(DiskError::Windows("invalid handle".into()));
        }

        // Set the file pointer to the desired offset.
        // SAFETY: SetFilePointerEx takes a valid HANDLE, a signed 64-bit
        // offset, an optional output pointer, and a move method.
        let pos = offset as i64;
        let ok = unsafe { SetFilePointerEx(h.0, pos, None, FILE_BEGIN) };
        if ok.is_err() {
            return Err(DiskError::Windows(format!(
                "SetFilePointerEx to {offset} failed"
            )));
        }

        // Read synchronously (no OVERLAPPED).
        let mut bytes_read: u32 = 0;
        // SAFETY: ReadFile takes a valid HANDLE, a writable buffer, an
        // optional pointer to receive bytes read, and an optional
        // OVERLAPPED pointer. We pass None for OVERLAPPED (synchronous).
        let ok = unsafe { ReadFile(h.0, Some(buf), Some(&mut bytes_read), None) };
        if ok.is_err() || bytes_read as usize != buf.len() {
            return Err(DiskError::Windows(format!(
                "ReadFile at offset {offset} got {bytes_read} of {} bytes",
                buf.len()
            )));
        }
        Ok(())
    }

    fn size(&self) -> u64 {
        self.size
    }
}

#[async_trait]
impl DiskSource for WindowsRawSource {
    async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<Bytes> {
        if len > crate::source::MAX_SINGLE_READ {
            return Err(DiskError::RequestTooLarge {
                len,
                cap: crate::source::MAX_SINGLE_READ,
            });
        }
        let mut buf = vec![0u8; len as usize];
        <Self as ReadAt>::read_at(self, offset, &mut buf)?;
        Ok(Bytes::from(buf))
    }

    async fn geometry(&self) -> DiskResult<DiskGeometry> {
        Ok(self.geometry.clone())
    }

    fn as_read_at(&self) -> Arc<dyn ReadAt> {
        Arc::new(WindowsRawReadAt {
            handle: Arc::clone(&self.handle),
            size: self.size,
        })
    }
}

/// Read handle sharing the source's device handle (see
/// FileBackedReadAt). Closes the handle when the last owner goes away
/// (SendHandle's Drop).
pub struct WindowsRawReadAt {
    handle: Arc<Mutex<SendHandle>>,
    size: u64,
}

impl ReadAt for WindowsRawReadAt {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        if offset.saturating_add(buf.len() as u64) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len: buf.len() as u64,
                source_size: self.size,
            });
        }
        let h = self.handle.lock();
        // SAFETY: identical call sequence to WindowsRawSource::read_at.
        let ok = unsafe { SetFilePointerEx(h.0, offset as i64, None, FILE_BEGIN) };
        if ok.is_err() {
            return Err(DiskError::Windows(format!(
                "SetFilePointerEx to {offset} failed"
            )));
        }
        let mut bytes_read: u32 = 0;
        let ok = unsafe { ReadFile(h.0, Some(buf), Some(&mut bytes_read), None) };
        if ok.is_err() || bytes_read as usize != buf.len() {
            return Err(DiskError::Windows(format!(
                "ReadFile at offset {offset} got {bytes_read} of {} bytes",
                buf.len()
            )));
        }
        Ok(())
    }

    fn size(&self) -> u64 {
        self.size
    }
}
