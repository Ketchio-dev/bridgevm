// Source-wiring regression; real callback completion still needs a live run.
#[test]
fn async_fences_use_the_renderer_global_dispatcher() {
    let source = include_str!("../venus_start_trace_capset_cou.rs");
    let poll = source
        .split("fn poll_fences(&mut self) {")
        .nth(1)
        .unwrap()
        .split("fn drain_completed_fences_into")
        .next()
        .unwrap();
    assert!(poll.contains("virgl_renderer_poll();"));
    assert!(!poll.contains("virgl_renderer_context_poll("));
    for protocol in [VirtioGpuRendererProtocol::Venus, VirtioGpuRendererProtocol::Virgl] {
        assert_ne!(protocol.init_flags() & VIRGL_RENDERER_ASYNC_FENCE_CB, 0);
        assert_ne!(protocol.init_flags() & VIRGL_RENDERER_THREAD_SYNC, 0);
    }
}
