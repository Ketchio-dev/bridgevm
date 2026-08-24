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
