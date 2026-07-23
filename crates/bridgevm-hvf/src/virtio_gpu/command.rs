//! Routing of control and cursor requests to the per-command handlers.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::CtrlHdr3d;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_CREATE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DESTROY;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET_INFO;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D;

impl VirtioGpu {
    pub(crate) fn handle_cursor_request_into(&mut self, request: &[u8], out: &mut Vec<u8>) {
        let hdr = CtrlHdr::parse(request);
        match hdr.map(|h| h.typ) {
            Some(VIRTIO_GPU_CMD_UPDATE_CURSOR | VIRTIO_GPU_CMD_MOVE_CURSOR) => {
                response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            }
            _ => response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr),
        }
    }

    pub(crate) fn handle_control_request_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        out: &mut Vec<u8>,
    ) {
        let Some(hdr) = CtrlHdr::parse(request) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, None);
            return;
        };
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_DISPLAY_INFO => self.response_display_info_into(Some(hdr), out),
            VIRTIO_GPU_CMD_GET_EDID => self.response_edid_into(Some(hdr), out),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
                self.resource_create_2d_into(request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_UNREF => self.resource_unref_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
                self.attach_backing_into(mem, request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING => {
                self.detach_backing_into(request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_SET_SCANOUT => self.set_scanout_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => self.set_scanout_blob_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
                self.transfer_to_host_2d_into(mem, request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_FLUSH => self.resource_flush_into(mem, request, Some(hdr), out),
            VIRTIO_GPU_CMD_GET_CAPSET_INFO
            | VIRTIO_GPU_CMD_GET_CAPSET
            | VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
            | VIRTIO_GPU_CMD_CTX_CREATE
            | VIRTIO_GPU_CMD_CTX_DESTROY
            | VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE
            | VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE
            | VIRTIO_GPU_CMD_RESOURCE_CREATE_3D
            | VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D
            | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
            | VIRTIO_GPU_CMD_SUBMIT_3D
            | VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
            | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
                let hdr3d = CtrlHdr3d::parse(request).unwrap();
                if hdr3d.typ == VIRTIO_GPU_CMD_CTX_DESTROY {
                    if let Some(resource_id) = self
                        .blob_scanout
                        .as_ref()
                        .map(|scanout| scanout.resource_id)
                    {
                        if self.three_d.ctx_has_resource(hdr3d.ctx_id, resource_id) {
                            self.unbind_blob_scanout();
                        }
                    }
                }
                if !self
                    .three_d
                    .handle_with_mem_into(Some(mem), request, hdr3d, out)
                {
                    virtio_gpu_3d::response_hdr_into(
                        out,
                        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_UNSPEC,
                        Some(hdr3d),
                    );
                }
            }
            _ => response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr)),
        }
    }
}
