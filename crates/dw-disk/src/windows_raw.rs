//! Windows raw-disk access via `DeviceIoControl` + `FSCTL_ALLOW_EXTENDED_DASD_IO`.
//!
//! Only compiled on Windows. The rest of the codebase talks to disks
//! through the [`DiskSource`](crate::DiskSource) trait, so this module is
//! the only place that has to deal with Windows HANDLEs.

#![cfg(windows)]
#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::OVERLAPPED;

use crate::source::{DiskSource, ReadAt};
use crate::{DiskError, DiskGeometry, DiskResult};

pub struct WindowsRawSource {
    handle: Mutex<HANDLE>,
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
        // string and standard flags. The returned handle is checked.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path_wide.as_ptr()),
                windows::Win32::Storage::FileSystem::GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(DiskError::Windows(format!(
                "CreateFileW failed for {path:?}"
            )));
        }
        let size = query_size(handle)?;
        let geometry = DiskGeometry {
            total_bytes: size,
            bytes_per_sector: 512,
            sectors_per_track: 0,
            heads: 0,
            cylinders: 0,
        };
        Ok(Self {
            handle: Mutex::new(handle),
            size,
            geometry,
        })
    }
}

impl Drop for WindowsRawSource {
    fn drop(&mut self) {
        let h = self.handle.lock();
        if *h != INVALID_HANDLE_VALUE {
            let _ = unsafe { CloseHandle(*h) };
        }
    }
}

fn query_size(handle: HANDLE) -> DiskResult<u64> {
    let mut size = windows::Win32::Foundation::LARGE_INTEGER(0);
    let ok = unsafe { windows::Win32::Storage::FileSystem::GetFileSizeEx(handle, &mut size) };
    if ok.is_err() {
        return Err(DiskError::Windows("GetFileSizeEx failed".into()));
    }
    Ok(size.0 as u64)
}

impl ReadAt for WindowsRawSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        let h = self.handle.lock();
        if *h == INVALID_HANDLE_VALUE {
            return Err(DiskError::Windows("invalid handle".into()));
        }
        let overlapped = OVERLAPPED {
            Internal: 0,
            InternalHigh: 0,
            Anonymous: windows::Win32::System::IO::DUMMYUNIONNAME {
                Anonymous: windows::Win32::System::IO::DUMMYSTRUCTNAME {
                    Offset: (offset & 0xFFFF_FFFF) as u32,
                    OffsetHigh: (offset >> 32) as u32,
                },
            },
            hEvent: windows::Win32::Foundation::HANDLE(std::ptr::null_mut()),
        };
        let mut bytes_read: u32 = 0;
        let ok = unsafe {
            windows::Win32::Storage::FileSystem::ReadFile(
                *h,
                Some(buf),
                &mut bytes_read,
                Some(&{ overlapped } as *const _ as *mut _),
            )
        };
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
        let mut buf = vec![0u8; len as usize];
        <Self as ReadAt>::read_at(self, offset, &mut buf)?;
        Ok(Bytes::from(buf))
    }

    async fn geometry(&self) -> DiskResult<DiskGeometry> {
        Ok(self.geometry.clone())
    }

    fn as_read_at(&self) -> Arc<dyn ReadAt> {
        // Windows HANDLE cannot be cheaply duplicated. Callers that need
        // a long-lived ReadAt should hold their own Arc<WindowsRawSource>.
        // The cross-platform code paths use FileBackedSource for tests.
        unimplemented!("callers should hold their own Arc<WindowsRawSource>")
    }
}
