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
        self.submits = self.submits.saturating_add(1);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }
}
