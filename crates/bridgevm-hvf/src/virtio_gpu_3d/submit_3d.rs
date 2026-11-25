//! SUBMIT_3D command-buffer validation and hand-off to the renderer.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_trace::venus_start_trace_enabled;

// Device-side storage for the diagnostic belonging to the command being traced.

impl VirtioGpu3d {
    pub(crate) fn clear_submit_diagnostic(&mut self) {
        self.last_submit_diagnostic = None;
    }

    pub(crate) fn set_submit_diagnostic(&mut self, diagnostic: Option<Submit3dDiagnostic>) {
        self.last_submit_diagnostic = diagnostic;
    }

    pub(crate) fn take_submit_diagnostic(&mut self) -> Option<Submit3dDiagnostic> {
        self.last_submit_diagnostic.take()
    }
}
// Renderer hand-off and virtio response for a validated live-context submit.

impl VirtioGpu3d {
    pub(crate) fn dispatch_submit(&mut self, cmdbuf: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let result = backend.submit_3d(hdr.ctx_id, cmdbuf);
        self.set_submit_diagnostic(result.diagnostic);
        if !result.accepted {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        if let Some(wake) = &self.venus_work_wake {
            if !cmdbuf.is_empty()
                && self.live_contexts.get(&hdr.ctx_id) == Some(&VIRTIO_GPU_CAPSET_VENUS)
            {
                wake.submitted();
            }
        }
        self.submits = self.submits.saturating_add(1);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }
}
// Guest-backed scanout copies answered without the renderer.

impl VirtioGpu3d {
    /// Local scanout copies short-circuit for BOTH live and pre-context
    /// submits. The 2026 neptune-era KMD wraps its GDI shadow->primary blt in a
    /// real virgl context (CTX_CREATE "virgl-gdi-blt" + SUBMIT
    /// RESOURCE_COPY_REGION); the primary lives in guest backing as a local
    /// scanout resource that virglrenderer has never seen, so forwarding the
    /// copy yields "Illegal resource" (b1-kmd-094334). try_local_resource_copies
    /// only claims buffers made entirely of copy commands whose resources are
    /// all locally backed; venus and real 3D submits fall through unchanged.
    pub(crate) fn dispatch_local_resource_copies(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        cmdbuf: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) -> bool {
        let Some(mem) = mem else { return false };
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
                true
            }
            LocalResourceCopyResult::Invalid => {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                true
            }
            LocalResourceCopyResult::NotApplicable => false,
        }
    }
}

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
        if self.dispatch_local_resource_copies(mem, cmdbuf, hdr, out) {
            return;
        }
        if !self.live_contexts.contains_key(&hdr.ctx_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.dispatch_submit(cmdbuf, hdr, out);
    }
}
