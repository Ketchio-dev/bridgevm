//! 3D context lifecycle and per-context resource attach/detach bookkeeping.

use super::*;

impl VirtioGpu3d {
    pub fn has_live_context(&self, ctx_id: u32) -> bool {
        self.live_contexts.contains(&ctx_id)
    }

    pub fn ctx_has_resource(&self, ctx_id: u32, resource_id: u32) -> bool {
        self.ctx_resources
            .get(&ctx_id)
            .is_some_and(|resources| resources.contains(&resource_id))
    }

    pub(crate) fn ctx_create_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if request.len() < CTX_CREATE_LEN
            || hdr.ctx_id == 0
            || self.live_contexts.contains(&hdr.ctx_id)
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
        let context_init = read_le_u32(request, 28).unwrap_or(0);
        let name = &request[32..32 + nlen];
        if !backend.ctx_create(hdr.ctx_id, context_init, name) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.live_contexts.insert(hdr.ctx_id);
        self.ctx_resources.entry(hdr.ctx_id).or_default();
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn ctx_destroy_into(&mut self, hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if !self.live_contexts.remove(&hdr.ctx_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.ctx_resources.remove(&hdr.ctx_id);
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_destroy(hdr.ctx_id);
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn ctx_attach_resource_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < CTX_RESOURCE_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.insert(resource_id);
        }
        if !self.local_3d_backing.contains_key(&resource_id) {
            if let Some(backend) = self.backend.as_mut() {
                backend.ctx_attach_resource(hdr.ctx_id, resource_id);
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn ctx_detach_resource_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < CTX_RESOURCE_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.remove(&resource_id);
        }
        if !self.local_3d_backing.contains_key(&resource_id) {
            if let Some(backend) = self.backend.as_mut() {
                backend.ctx_detach_resource(hdr.ctx_id, resource_id);
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }
}
