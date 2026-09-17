//! Shared-memory framebuffer export sink: mmap file, header/seq protocol, teardown.

use std::fs::File;
use std::path::PathBuf;

pub(crate) struct FbSink {
    pub(crate) path: PathBuf,
    pub(crate) file: Option<File>,
    pub(crate) map: *mut u8,
    pub(crate) map_len: usize,
    pub(crate) capacity: usize,
    pub(crate) seq: u64,
}

// The device owns FbSink single-threadedly on the vCPU thread. The raw mmap
// pointer is never shared across threads; this only satisfies VirtioGpu's Send bound.
unsafe impl Send for FbSink {}

impl Drop for FbSink {
    fn drop(&mut self) {
        if !self.map.is_null() {
            unsafe {
                libc::munmap(self.map.cast(), self.map_len);
            }
            self.map = std::ptr::null_mut();
            self.map_len = 0;
            self.capacity = 0;
        }
    }
}

impl std::fmt::Debug for FbSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FbSink")
            .field("path", &self.path)
            .field("capacity", &self.capacity)
            .field("seq", &self.seq)
            .finish()
    }
}

impl FbSink {
    pub(crate) fn from_env() -> Option<FbSink> {
        let path = std::env::var_os("BRIDGEVM_DISPLAY_EXPORT_FB")?;
        if path.is_empty() {
            return None;
        }

        Some(FbSink {
            path: PathBuf::from(path),
            file: None,
            map: std::ptr::null_mut(),
            map_len: 0,
            capacity: 0,
            seq: 0,
        })
    }
}
