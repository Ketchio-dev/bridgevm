// Renderer SUBMIT_3D status and bounded diagnostic propagation.

impl VenusBackend {
    pub(crate) fn submit_renderer_command(
        &mut self,
        ctx_id: u32,
        cmdbuf: &[u8],
    ) -> Submit3dResult {
        host_gl::rebind_last_context();
        let ndw = cmdbuf.len().div_ceil(4);
        let ret = if cmdbuf.is_empty() {
            unsafe { virgl_renderer_submit_cmd(std::ptr::null_mut(), ctx_id as c_int, 0) }
        } else {
            unsafe {
                virgl_renderer_submit_cmd(
                    cmdbuf.as_ptr() as *mut c_void,
                    ctx_id as c_int,
                    ndw as c_int,
                )
            }
        };
        if ret == 0 {
            return Submit3dResult::accepted();
        }

        let diagnostic = Some(take_submit_diagnostic(ret));
        eprintln!(
            "{}: submit_cmd ctx={ctx_id} bytes={} ret={ret}",
            self.protocol.label(),
            cmdbuf.len()
        );
        // The legacy VirGL path records renderer diagnostics without turning
        // a vrend context error into a virtio command error.
        Submit3dResult {
            accepted: self.protocol == VirtioGpuRendererProtocol::Virgl,
            diagnostic,
        }
    }
}
