//! SUBMIT_3D command-buffer validation and hand-off to the renderer.

use super::*;
include!("submit_diagnostic_state.rs");
include!("submit_dispatch.rs");
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_trace::venus_start_trace_enabled;

impl VirtioGpu3d {
    pub(crate) fn submit_3d_into(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        self.clear_submit_diagnostic();
        if self.backend.is_none() {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if request.len() < SUBMIT_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let size = read_le_u32(request, 24).unwrap_or(0) as usize;
        // The Windows VirGL driver uses an empty context-0 submit as a queue
        // synchronization no-op. It has no renderer payload or context state to
        // validate, so acknowledge it without calling virglrenderer.
        if size == 0 && hdr.ctx_id == 0 {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
            return;
        }
        if size > MAX_SUBMIT_3D_BYTES || request.len().saturating_sub(SUBMIT_3D_LEN) < size {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let cmdbuf = &request[SUBMIT_3D_LEN..SUBMIT_3D_LEN + size];
        // Local scanout copies short-circuit for BOTH live and pre-context
        // submits. The 2026 neptune-era KMD wraps its GDI shadow->primary
        // blt in a real virgl context (CTX_CREATE "virgl-gdi-blt" + SUBMIT
        // RESOURCE_COPY_REGION); the primary lives in guest backing as a
        // local scanout resource that virglrenderer has never seen, so
        // forwarding the copy yields "Illegal resource" (b1-kmd-094334).
        // try_local_resource_copies only claims buffers made entirely of
        // copy commands whose resources are all locally backed; venus and
        // real 3D submits fall through to the renderer unchanged.
        if let Some(mem) = mem {
            match self.try_local_resource_copies(mem, cmdbuf) {
                LocalResourceCopyResult::Copied { regions } => {
                    self.local_copy_submits = self.local_copy_submits.saturating_add(1);
                    self.submits = self.submits.saturating_add(1);
                    if venus_start_trace_enabled() && self.local_copy_submits == 1 {
                        println!(
                            "venus-start: local resource_copy_region ctx={} regions={regions}",
                            hdr.ctx_id
                        );
                    }
                    response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
                    return;
                }
                LocalResourceCopyResult::Invalid => {
                    response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                    return;
                }
                LocalResourceCopyResult::NotApplicable => {}
            }
        }
        if !self.live_contexts.contains(&hdr.ctx_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.dispatch_submit(cmdbuf, hdr, out);
    }
}
