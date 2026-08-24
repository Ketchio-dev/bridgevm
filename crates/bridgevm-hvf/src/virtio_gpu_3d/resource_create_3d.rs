//! RESOURCE_CREATE_3D decode, renderer/local classification and registration.

use super::resource_3d::*;
use super::*;
use crate::virtio_gpu_trace::venus_start_trace_enabled;

impl VirtioGpu3d {
    pub(crate) fn resource_create_3d_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_CREATE_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let args = Create3dArgs {
            resource_id: read_le_u32(request, 24).unwrap_or(0),
            target: read_le_u32(request, 28).unwrap_or(0),
            format: read_le_u32(request, 32).unwrap_or(0),
            bind: read_le_u32(request, 36).unwrap_or(0),
            width: read_le_u32(request, 40).unwrap_or(0),
            height: read_le_u32(request, 44).unwrap_or(0),
            depth: read_le_u32(request, 48).unwrap_or(0),
            array_size: read_le_u32(request, 52).unwrap_or(0),
            last_level: read_le_u32(request, 56).unwrap_or(0),
            nr_samples: read_le_u32(request, 60).unwrap_or(0),
            flags: read_le_u32(request, 64).unwrap_or(0),
        };
        if args.resource_id == 0 || self.resource_exists(args.resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        // The Venus WDDM KMD creates its shared primary before the UMD has
        // created the context whose numeric id is used by the subsequent
        // CTX_ATTACH_RESOURCE.  Keep that narrowly identified display resource
        // in guest backing even when the renderer also supports legacy virgl
        // resources; otherwise the early attach is lost inside virglrenderer.
        // Non-scanout render targets continue through the renderer below.
        let local_scanout = self.backend.is_some() && is_local_scanout_resource(args);
        let created = local_scanout
            || self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.create_3d(args));
        if !created {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.resource_3d_ids.insert(args.resource_id);
        self.set_backend_backing(args.resource_id, false);
        self.resource_3d_info.insert(args.resource_id, args);
        if crate::virtio_gpu_trace::venus_start_trace_enabled() {
            println!(
                "venus-start: create_3d res={} target={} format={} bind={:#x} {}x{} local={}",
                args.resource_id,
                args.target,
                args.format,
                args.bind,
                args.width,
                args.height,
                local_scanout
            );
        }
        if local_scanout {
            self.local_3d_backing.insert(args.resource_id, Vec::new());
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: local display resource_create_3d res={} format={} bind={:#x} size={}x{}",
                    args.resource_id, args.format, args.bind, args.width, args.height
                );
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }
}
