//! Shared-memory framebuffer export sink: mmap file, header/seq protocol, teardown.

use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

/// One writer of a display file. Writers of the same file in one process
/// (the virtio-gpu device and the ramfb exporter) are made with [`FbSink::share`]
/// and take turns under one lock and one sequence; each maps the file itself.
pub(crate) struct FbSink {
    pub(crate) path: PathBuf,
    pub(crate) file: Option<File>,
    pub(crate) map: *mut u8,
    pub(crate) map_len: usize,
    pub(crate) capacity: usize,
    pub(crate) shared: Arc<Mutex<FbShared>>,
    pub(crate) writer: u64,
}

/// What every writer of one file shares. A frame is written while this is
/// locked, so the reader's seqlock never sees two writers at once.
#[derive(Debug, Default)]
pub(crate) struct FbShared {
    /// Sequence after the last completed publish: 0 before the first, then even.
    pub(crate) seq: u64,
    /// The writer of the file's newest frame, and when it was published.
    pub(crate) newest: Option<(u64, Instant)>,
}

// Each FbSink is used by one thread at a time (the device's on a vCPU, the
// ramfb exporter's on its own thread). Its raw mmap pointer is never shared
// across threads; this only satisfies the Send bounds of its owners.
unsafe impl Send for FbSink {}

impl Drop for FbSink {
    fn drop(&mut self) {
        self.reset_mapping();
    }
}

impl std::fmt::Debug for FbSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FbSink")
            .field("path", &self.path)
            .field("capacity", &self.capacity)
            .field("writer", &self.writer)
            .finish()
    }
}

impl FbSink {
    pub(crate) fn from_env() -> Option<FbSink> {
        let path = std::env::var_os("BRIDGEVM_DISPLAY_EXPORT_FB")?;
        (!path.is_empty()).then(|| FbSink::at_path(PathBuf::from(path)))
    }

    /// A sink for `path`. Nothing is created or mapped until the first write.
    pub(crate) fn at_path(path: PathBuf) -> FbSink {
        FbSink::with_shared(path, Arc::default())
    }

    /// Another writer of the same file, sharing this one's lock and sequence.
    pub(crate) fn share(&self) -> FbSink {
        FbSink::with_shared(self.path.clone(), Arc::clone(&self.shared))
    }

    fn with_shared(path: PathBuf, shared: Arc<Mutex<FbShared>>) -> FbSink {
        static NEXT_WRITER: AtomicU64 = AtomicU64::new(1);
        FbSink {
            path,
            file: None,
            map: std::ptr::null_mut(),
            map_len: 0,
            capacity: 0,
            shared,
            writer: NEXT_WRITER.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub(crate) fn lock_shared(&self) -> MutexGuard<'_, FbShared> {
        FbShared::lock(&self.shared)
    }
}

impl FbShared {
    /// A writer that panicked mid-frame left an odd sequence, which the next
    /// frame's even one replaces; the state stays usable.
    pub(crate) fn lock(shared: &Mutex<FbShared>) -> MutexGuard<'_, FbShared> {
        shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
