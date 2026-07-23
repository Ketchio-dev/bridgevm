//! Continuation of the `virtio_gpu_f_virgl` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;

impl VirtioGpu3d {
    pub fn local_3d_backing(&self, resource_id: u32) -> Option<&[BlobMemEntry]> {
        self.local_3d_backing.get(&resource_id).map(Vec::as_slice)
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

    pub fn attach_3d_backing(
        &mut self,
        mem: &dyn GuestMemoryMut,
        resource_id: u32,
        backing: &[BlobMemEntry],
    ) -> bool {
        if !self.resource_3d_ids.contains(&resource_id) || backing.is_empty() {
            return false;
        }
        self.host_iovecs_scratch.clear();
        if !resolve_blob_iovecs_into(mem, backing, &mut self.host_iovecs_scratch) {
            return false;
        }
        if let Some(local_backing) = self.local_3d_backing.get_mut(&resource_id) {
            let Some(info) = self.resource_3d_info.get(&resource_id) else {
                self.host_iovecs_scratch.clear();
                return false;
            };
            let required = u64::from(info.width)
                .checked_mul(u64::from(info.height))
                .and_then(|pixels| pixels.checked_mul(4));
            let available = backing.iter().fold(0u64, |total, entry| {
                total.saturating_add(u64::from(entry.len))
            });
            if !matches!(required, Some(required) if available >= required) {
                self.host_iovecs_scratch.clear();
                return false;
            }
            local_backing.clear();
            local_backing.extend_from_slice(backing);
            self.host_iovecs_scratch.clear();
            return true;
        }
        let attached = self
            .backend
            .as_mut()
            .is_some_and(|backend| backend.attach_backing(resource_id, &self.host_iovecs_scratch));
        self.host_iovecs_scratch.clear();
        attached
    }

    pub fn detach_3d_backing(&mut self, resource_id: u32) -> bool {
        if let Some(backing) = self.local_3d_backing.get_mut(&resource_id) {
            backing.clear();
            return true;
        }
        self.resource_3d_ids.contains(&resource_id)
            && self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.detach_backing(resource_id))
    }

    pub fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        let mut completed = Vec::new();
        self.drain_completed_fences_into(&mut completed);
        completed
    }

    pub fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        self.drain_completed_fences_inner(out, false);
    }

    pub fn drain_completed_fences_after_queue_into(&mut self, out: &mut Vec<CompletedFence>) {
        self.drain_completed_fences_inner(out, true);
    }

    pub(crate) fn drain_completed_fences_inner(
        &mut self,
        out: &mut Vec<CompletedFence>,
        after_queue: bool,
    ) {
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        // Venus on macOS retires fences synchronously: polling the backend may
        // invoke the fence callback inline, then drain_completed_fences takes
        // the callbacks queued by that poll.
        if after_queue {
            backend.poll_fences_after_queue();
        } else {
            backend.poll_fences();
        }
        let start = out.len();
        backend.drain_completed_fences_into(out);
        self.fences_completed = self
            .fences_completed
            .saturating_add((out.len() - start) as u64);
    }

    pub fn create_fence(&mut self, fence: CompletedFence) -> bool {
        let Some(backend) = self.backend.as_mut() else {
            return false;
        };
        backend.create_fence(fence.ctx_id, fence.ring_idx, fence.fence_id)
    }

    pub fn handle(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Option<Vec<u8>> {
        self.handle_with_mem(None, request, hdr)
    }

    pub fn handle_with_mem(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
    ) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        self.handle_with_mem_into(mem, request, hdr, &mut out)
            .then_some(out)
    }

    pub fn handle_with_mem_into(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) -> bool {
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_CAPSET_INFO => self.get_capset_info_into(request, hdr, out),
            VIRTIO_GPU_CMD_GET_CAPSET => self.get_capset_into(request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
                self.resource_create_blob_into(mem, request, hdr, out)
            }
            VIRTIO_GPU_CMD_CTX_CREATE => self.ctx_create_into(request, hdr, out),
            VIRTIO_GPU_CMD_CTX_DESTROY => self.ctx_destroy_into(hdr, out),
            VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => self.ctx_attach_resource_into(request, hdr, out),
            VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => self.ctx_detach_resource_into(request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => self.resource_create_3d_into(request, hdr, out),
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D => self.transfer_3d_into(request, hdr, true, out),
            VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => self.transfer_3d_into(request, hdr, false, out),
            VIRTIO_GPU_CMD_SUBMIT_3D => self.submit_3d_into(mem, request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => self.resource_map_blob_into(request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => self.resource_unmap_blob_into(request, hdr, out),
            _ => return false,
        }
        true
    }

    pub(crate) fn get_capset_info_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        let Some(backend) = self.backend.as_mut() else {
            venus_start_trace_reject("get_capset_info", "no 3D backend");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(index) = read_le_u32(request, 24) else {
            venus_start_trace_reject("get_capset_info", "short request");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(info) = backend.capset_info(index) else {
            venus_start_trace_reject("get_capset_info", "backend has no capset at index");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_CAPSET_INFO, Some(hdr));
        out.extend_from_slice(&info.capset_id.to_le_bytes());
        out.extend_from_slice(&info.max_version.to_le_bytes());
        out.extend_from_slice(&info.max_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    pub(crate) fn get_capset_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(capset_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(version) = read_le_u32(request, 28) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let response_start = out.len();
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_CAPSET, Some(hdr));
        if !backend.capset_into(capset_id, version, out) {
            venus_start_trace_reject("get_capset", "backend capset_into failed");
            out.truncate(response_start);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
    }
}
