//! Thread-confined scanout helpers for the Venus renderer worker.

use super::*;
use crate::virtio_gpu_3d::{ScanoutPresentResult, VirtioGpu3dBackend};

impl ThreadedVenusBackend {
    pub(crate) fn read_scanout_on_worker(
        &self,
        resource_id: u32,
        width: u32,
        height: u32,
        out: &mut [u8],
    ) -> bool {
        let out_address = out.as_mut_ptr() as usize;
        let out_len = out.len();
        self.call(move |backend| {
            // call() blocks, so the output remains alive and exclusively borrowed.
            let out = unsafe { std::slice::from_raw_parts_mut(out_address as *mut u8, out_len) };
            backend.scanout_read(resource_id, width, height, out)
        })
    }

    pub(crate) fn present_scanout_on_worker(
        &self,
        resource_id: u32,
        width: u32,
        height: u32,
        blit_iosurface: bool,
        readback: Option<&mut [u8]>,
    ) -> ScanoutPresentResult {
        let (readback_address, readback_len) =
            readback.map_or((0, 0), |out| (out.as_mut_ptr() as usize, out.len()));
        self.call(move |backend| {
            // call() blocks across both renderer operations, preserving the borrow.
            let readback = (readback_len != 0).then(|| unsafe {
                std::slice::from_raw_parts_mut(readback_address as *mut u8, readback_len)
            });
            backend.scanout_present(resource_id, width, height, blit_iosurface, readback)
        })
    }

    /// Start a present without waiting for it.
    ///
    /// The scratch buffer is MOVED to the worker and handed back with the
    /// result. The blocking path can lend a borrowed buffer because it waits;
    /// this one cannot, so ownership transfer is what keeps the buffer alive
    /// for exactly as long as the renderer is writing into it.
    pub(crate) fn start_present_on_worker(
        &self,
        resource_id: u32,
        width: u32,
        height: u32,
        blit_iosurface: bool,
        mut readback: Option<Vec<u8>>,
    ) -> std::sync::mpsc::Receiver<(ScanoutPresentResult, Option<Vec<u8>>)> {
        self.dispatch(move |backend| {
            let result = backend.scanout_present(
                resource_id,
                width,
                height,
                blit_iosurface,
                readback.as_deref_mut(),
            );
            (result, readback)
        })
    }
}
