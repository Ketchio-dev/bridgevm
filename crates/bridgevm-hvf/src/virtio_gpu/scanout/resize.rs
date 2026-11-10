//! Requested-to-active display geometry transition helpers.

use super::*;

impl VirtioGpu {
    pub(super) fn scanout_geometry_in_range(&self, width: u32, height: u32) -> bool {
        width > 0
            && height > 0
            && width <= self.width.max(self.requested_width)
            && height <= self.height.max(self.requested_height)
    }

    /// Commit a guest mode switch only when its new scanout exactly matches the
    /// geometry most recently requested by the host.
    pub(super) fn adopt_requested_display_resolution(&mut self, width: u32, height: u32) {
        if (width != self.requested_width || height != self.requested_height)
            || (width == self.width && height == self.height)
        {
            return;
        }
        self.width = width;
        self.height = height;
        // A new buffer, not `clear` + `resize`: those wrote every byte of the
        // grown buffer under the platform lock, 1.4-1.5 ms at 3456x2234
        // (SET_SCANOUT in the virtio-gpu trace). `vec![0; n]` is calloc, whose
        // fresh pages are zeroed as the next flush, unlocked, first writes them.
        self.scanout = vec![0; scanout_len(width, height)];
    }
}
