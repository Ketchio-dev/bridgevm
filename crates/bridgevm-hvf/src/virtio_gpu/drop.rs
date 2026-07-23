//! Split out of virtio_gpu.rs to keep files under 850 lines.

use super::*;

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
