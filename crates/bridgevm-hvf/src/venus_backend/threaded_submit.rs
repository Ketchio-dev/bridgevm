// Thread-confined renderer forwarding for SUBMIT_3D.

impl ThreadedVenusBackend {
    pub(crate) fn submit_renderer_command(
        &self,
        ctx_id: u32,
        cmdbuf: &[u8],
    ) -> Submit3dResult {
        let cmdbuf_address = cmdbuf.as_ptr() as usize;
        let cmdbuf_len = cmdbuf.len();
        self.call(move |backend| {
            // call() blocks, so the immutable command buffer stays alive.
            let cmdbuf =
                unsafe { std::slice::from_raw_parts(cmdbuf_address as *const u8, cmdbuf_len) };
            backend.submit_3d(ctx_id, cmdbuf)
        })
    }
}
