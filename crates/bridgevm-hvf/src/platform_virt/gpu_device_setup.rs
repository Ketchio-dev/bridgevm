//! virtio-gpu construction: 2D vs venus 3D backend, scanout readback and vblank pacing.

use super::*;
use crate::virtio_gpu::VblankWakeState;
use crate::virtio_gpu::VirtioPciGpu;
use std::sync::Arc;

pub(crate) fn make_virtio_gpu() -> VirtioPciGpu {
    let (width, height) = virtio_gpu_resolution_from_env();
    if env_flag("BRIDGEVM_VIRTIO_GPU_3D") {
        #[cfg(feature = "venus")]
        {
            let direct = env_flag("BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER");
            let backend = if direct {
                crate::venus_backend::VenusBackend::new().map(|backend| {
                    let protocol = backend.protocol();
                    (
                        protocol,
                        Box::new(backend) as Box<dyn crate::virtio_gpu_3d::VirtioGpu3dBackend>,
                    )
                })
            } else {
                crate::venus_backend::ThreadedVenusBackend::new().map(|backend| {
                    let protocol = backend.protocol();
                    (
                        protocol,
                        Box::new(backend) as Box<dyn crate::virtio_gpu_3d::VirtioGpu3dBackend>,
                    )
                })
            };
            match backend {
                Ok((protocol, backend)) => {
                    eprintln!(
                        "virtio-gpu: {} 3D backend enabled mode={}",
                        protocol.label(),
                        if direct { "direct-rebind" } else { "threaded" }
                    );
                    let mut gpu = VirtioPciGpu::with_3d_backend(width, height, backend);
                    let interval = virtio_gpu_3d_scanout_readback_interval_from_value(
                        std::env::var("BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS")
                            .ok()
                            .as_deref(),
                    );
                    gpu.set_3d_scanout_readback_interval(interval);
                    if env_flag("BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT") {
                        gpu.set_3d_scanout_deferred(true);
                        eprintln!("virtio-gpu: 3D scanout readback deferred off the flush path");
                    }
                    if env_flag("BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT") {
                        gpu.set_3d_scanout_iosurface(
                            true,
                            env_flag("BRIDGEVM_VIRTIO_GPU_IOSURFACE_VERIFY"),
                        );
                        eprintln!("virtio-gpu: 3D scanout IOSurface GPU blit enabled");
                    }
                    configure_virtio_gpu_vblank(&mut gpu);
                    eprintln!(
                        "virtio-gpu: 3D scanout readback pacing={}ms",
                        interval.as_millis()
                    );
                    return gpu;
                }
                Err(error) => {
                    panic!("virtio-gpu: requested 3D backend failed to initialize: {error}");
                }
            }
        }
        #[cfg(not(feature = "venus"))]
        {
            panic!(
                "virtio-gpu: BRIDGEVM_VIRTIO_GPU_3D requested but this probe was built without the venus feature"
            );
        }
    }
    let mut gpu = VirtioPciGpu::new(width, height);
    configure_virtio_gpu_vblank(&mut gpu);
    gpu
}

pub(crate) fn configure_virtio_gpu_vblank(gpu: &mut VirtioPciGpu) {
    let value = std::env::var("BRIDGEVM_VBLANK_HZ").ok();
    let interval = virtio_gpu_vblank_interval_from_value(value.as_deref());
    gpu.set_vblank_interval(interval);
    if !interval.is_zero() {
        gpu.set_vblank_wake(Arc::new(VblankWakeState::new()));
        eprintln!(
            "virtio-gpu: host vblank pacing interval={}ns",
            interval.as_nanos()
        );
    }
}
