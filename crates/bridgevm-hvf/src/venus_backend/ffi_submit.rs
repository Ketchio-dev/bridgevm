// BridgeVM virglrenderer submission ABI extensions.

unsafe extern "C" {
    pub(super) fn virgl_renderer_bridgevm_get_last_submit_diagnostic(
        out: *mut VirglSubmitDiagnostic,
    ) -> c_int;
}
