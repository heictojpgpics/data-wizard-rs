//! The pause gate — a `DiskSource` wrapper that stalls reads while a
//! shared flag is set.
//!
//! Pause semantics (the EaseUS Scan-Paused control): every read path
//! funnels through here, so the ENTIRE pipeline (FS metadata walks via
//! the sync [`ReadAt`] view, carving via `read_at_async`) freezes in
//! place without touching a single engine. The async path parks on a
//! tokio sleep-poll; the sync path parks on a 12ms spin-sleep, which
//! blocks at most one runtime worker — the same worker that would
//! otherwise be running the scan, so nothing else starves.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use crate::source::{DiskSource, ReadAt};
use crate::{DiskGeometry, DiskResult};

const SYNC_POLL: Duration = Duration::from_millis(12);
const ASYNC_POLL: Duration = Duration::from_millis(25);

/// A source whose reads stall while `flag` is set. Clone the flag out
/// before constructing to keep a handle for the unpause path.
pub struct PausableSource {
    inner: Arc<dyn DiskSource>,
    flag: Arc<AtomicBool>,
}

impl PausableSource {
    #[must_use]
    pub fn new(inner: Arc<dyn DiskSource>, flag: Arc<AtomicBool>) -> Self {
        Self { inner, flag }
    }

    async fn wait_async(&self) {
        while self.flag.load(Ordering::Relaxed) {
            tokio::time::sleep(ASYNC_POLL).await;
        }
    }
}

#[async_trait]
impl DiskSource for PausableSource {
    async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<bytes::Bytes> {
        self.wait_async().await;
        self.inner.read_at_async(offset, len).await
    }

    async fn geometry(&self) -> DiskResult<DiskGeometry> {
        // Cheap metadata call; no gate (probe must answer while paused).
        self.inner.geometry().await
    }

    fn as_read_at(&self) -> Arc<dyn ReadAt> {
        Arc::new(PausableReadAt {
            inner: self.inner.as_read_at(),
            flag: Arc::clone(&self.flag),
        })
    }
}

/// The gated sync view handed to filesystem parsers.
struct PausableReadAt {
    inner: Arc<dyn ReadAt>,
    flag: Arc<AtomicBool>,
}

impl ReadAt for PausableReadAt {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        while self.flag.load(Ordering::Relaxed) {
            std::thread::sleep(SYNC_POLL);
        }
        self.inner.read_at(offset, buf)
    }

    fn size(&self) -> u64 {
        self.inner.size()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]
    use super::*;
    use crate::DiskError;

    struct ConstSource(u64);

    #[async_trait]
    impl DiskSource for ConstSource {
        async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<bytes::Bytes> {
            if offset + len > self.0 {
                return Err(DiskError::OutOfRange {
                    offset,
                    len,
                    source_size: self.0,
                });
            }
            Ok(bytes::Bytes::from(vec![0u8; len as usize]))
        }
        async fn geometry(&self) -> DiskResult<DiskGeometry> {
            Ok(DiskGeometry {
                total_bytes: self.0,
                bytes_per_sector: 512,
                sectors_per_track: 63,
                heads: 255,
                cylinders: 1,
            })
        }
        fn as_read_at(&self) -> Arc<dyn ReadAt> {
            Arc::new(ConstReadAt(self.0))
        }
    }

    struct ConstReadAt(u64);
    impl ReadAt for ConstReadAt {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
            if offset + buf.len() as u64 > self.0 {
                return Err(DiskError::OutOfRange {
                    offset,
                    len: buf.len() as u64,
                    source_size: self.0,
                });
            }
            buf.fill(0);
            Ok(())
        }
        fn size(&self) -> u64 {
            self.0
        }
    }

    #[tokio::test]
    async fn async_read_gates_on_flag() {
        let flag = Arc::new(AtomicBool::new(true));
        let src = PausableSource::new(Arc::new(ConstSource(1024)), Arc::clone(&flag));

        // While paused: the read parks. Spawn it, then unpause.
        let task = tokio::spawn(async move { src.read_at_async(0, 16).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished());
        flag.store(false, Ordering::Relaxed);
        let buf = task.await.expect("join").expect("read");
        assert_eq!(buf.len(), 16);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sync_read_gates_on_flag() {
        let flag = Arc::new(AtomicBool::new(true));
        let src = PausableSource::new(Arc::new(ConstSource(1024)), Arc::clone(&flag));
        let read_at = src.as_read_at();

        let task = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 8];
            read_at.read_at(0, &mut buf)
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished());
        flag.store(false, Ordering::Relaxed);
        task.await.expect("join").expect("read");
    }

    #[tokio::test]
    async fn unpaused_reads_pass_through() {
        let flag = Arc::new(AtomicBool::new(false));
        let src = PausableSource::new(Arc::new(ConstSource(1024)), flag);
        let buf = src.read_at_async(512, 32).await.expect("read");
        assert_eq!(buf.len(), 32);
    }
}
