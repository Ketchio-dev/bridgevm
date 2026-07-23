//! Split test module.

use super::super::*;

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
