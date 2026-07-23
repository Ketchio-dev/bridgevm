//! Routing a decoded command to the responsible handler.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl VirtioGpu3d {
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
}
