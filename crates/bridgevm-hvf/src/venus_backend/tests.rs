use super::*;

// Source-wiring regression; real callback completion still needs a live run.
#[test]
fn async_fences_use_the_renderer_global_dispatcher() {
    let source = include_str!("venus_start_trace_capset_cou.rs");
    let poll = source
        .split("fn poll_fences(&mut self) {")
        .nth(1)
        .unwrap()
        .split("fn drain_completed_fences_into")
        .next()
        .unwrap();
    assert!(poll.contains("virgl_renderer_poll();"));
    assert!(!poll.contains("virgl_renderer_context_poll("));
    for protocol in [
        VirtioGpuRendererProtocol::Venus,
        VirtioGpuRendererProtocol::Virgl,
    ] {
        assert_ne!(protocol.init_flags() & VIRGL_RENDERER_ASYNC_FENCE_CB, 0);
        assert_ne!(protocol.init_flags() & VIRGL_RENDERER_THREAD_SYNC, 0);
    }
}
#[test]
fn venus_advertises_vulkan_and_windows_shadow_capsets() {
    let protocol = VirtioGpuRendererProtocol::Venus;
    assert_eq!(
        (0..4)
            .map(|index| protocol.capset_id_for_index(index))
            .collect::<Vec<_>>(),
        vec![
            Some(VIRTIO_GPU_CAPSET_VENUS),
            Some(VIRTIO_GPU_CAPSET_VIRGL),
            Some(VIRTIO_GPU_CAPSET_VIRGL2),
            None,
        ]
    );
    for capset_id in [
        VIRTIO_GPU_CAPSET_VENUS,
        VIRTIO_GPU_CAPSET_VIRGL,
        VIRTIO_GPU_CAPSET_VIRGL2,
    ] {
        assert!(protocol.supports_capset_id(capset_id));
    }
}

#[test]
fn venus_renderer_keeps_virgl_enabled_for_wddm_present() {
    let flags = VirtioGpuRendererProtocol::Venus.init_flags();
    assert_ne!(flags & VIRGL_RENDERER_VENUS, 0);
    assert_ne!(flags & VIRGL_RENDERER_RENDER_SERVER, 0);
    assert_eq!(flags & VIRGL_RENDERER_NO_VIRGL, 0);
}
