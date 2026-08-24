//! Refuse an unbacked transfer the way virglrenderer would: prefix first.

use super::*;

impl VirtioGpu3d {
    /// The renderer executes every command preceding the one it rejects and only
    /// then stops (`vrend_decode.c:2160-2170`), so refusing the whole buffer
    /// would silently drop object creations the guest never repeats. Dispatch
    /// the prefix first, then refuse, leaving the context exactly as far along
    /// as virglrenderer would have taken it.
    pub(crate) fn submit_rejected_before_renderer(
        &mut self,
        ctx_id: u32,
        cmdbuf: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) -> bool {
        let Some(diagnostic) = self.preflight_unbacked_buffer_transfer(ctx_id, cmdbuf) else {
            return false;
        };
        let prefix = diagnostic
            .command_offset_dwords
            .map_or(0, |dwords| dwords as usize * 4);
        if prefix > 0 {
            let mut discarded = Vec::new();
            self.dispatch_submit(&cmdbuf[..prefix], hdr, &mut discarded);
        }
        self.set_submit_diagnostic(Some(diagnostic));
        response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
        true
    }
}
