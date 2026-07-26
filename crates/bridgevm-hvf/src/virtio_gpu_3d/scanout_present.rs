//! Scanout presentation: blob map, CPU readback, IOSurface blit and its diagnostics.

use super::*;

impl VirtioGpu3d {
    pub fn scanout_map_blob(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob> {
        self.backend.as_mut()?.scanout_map(resource_id)
    }

    pub fn scanout_unmap_blob(&mut self, resource_id: u32) {
        if let Some(backend) = self.backend.as_mut() {
            backend.scanout_unmap(resource_id);
        }
    }

    pub fn read_3d_scanout(
        &mut self,
        resource_id: u32,
        width: u32,
        height: u32,
        out: &mut [u8],
    ) -> bool {
        let Some(info) = self.scanout_3d_info(resource_id) else {
            return false;
        };
        if width > info.width || height > info.height {
            return false;
        }
        self.backend
            .as_mut()
            .is_some_and(|backend| backend.scanout_read(resource_id, width, height, out))
    }

    pub fn blit_3d_scanout_iosurface(
        &mut self,
        resource_id: u32,
        width: u32,
        height: u32,
    ) -> Option<u32> {
        let info = self.scanout_3d_info(resource_id)?;
        if width > info.width || height > info.height {
            return None;
        }
        self.backend
            .as_mut()
            .and_then(|backend| backend.scanout_blit_iosurface(resource_id, width, height))
    }

    pub fn present_3d_scanout(
        &mut self,
        resource_id: u32,
        width: u32,
        height: u32,
        blit_iosurface: bool,
        readback: Option<&mut [u8]>,
    ) -> ScanoutPresentResult {
        let Some(info) = self.scanout_3d_info(resource_id) else {
            return ScanoutPresentResult::default();
        };
        if width > info.width || height > info.height {
            return ScanoutPresentResult::default();
        }
        self.backend
            .as_mut()
            .map_or_else(ScanoutPresentResult::default, |backend| {
                backend.scanout_present(resource_id, width, height, blit_iosurface, readback)
            })
    }

    /// Start a present the renderer executes off the caller's thread.
    /// `None` means the backend has no asynchronous path.
    pub fn scanout_present_start(
        &mut self,
        resource_id: u32,
        width: u32,
        height: u32,
        blit_iosurface: bool,
        readback: Option<Vec<u8>>,
    ) -> Option<ScanoutPresentPending> {
        let info = self.scanout_3d_info(resource_id)?;
        if width > info.width || height > info.height {
            return None;
        }
        self.backend.as_mut().and_then(|backend| {
            backend.scanout_present_start(resource_id, width, height, blit_iosurface, readback)
        })
    }

    pub fn scanout_present_poll(
        &mut self,
        pending: &mut ScanoutPresentPending,
        block: bool,
    ) -> Option<(ScanoutPresentResult, Option<Vec<u8>>)> {
        self.backend
            .as_mut()
            .and_then(|backend| backend.scanout_present_poll(pending, block))
    }

    pub fn scanout_iosurface_checksum(&mut self) -> Option<u64> {
        self.backend
            .as_mut()
            .and_then(|backend| backend.scanout_iosurface_checksum())
    }

    pub fn scanout_iosurface_dump(&mut self, path: &std::path::Path) -> bool {
        self.backend
            .as_mut()
            .is_some_and(|backend| backend.scanout_iosurface_dump(path))
    }
}
