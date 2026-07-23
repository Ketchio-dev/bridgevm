//! BRIDGEVM_* environment knobs and the defaults they fall back to.

use std::time::Duration;

#[cfg(any(feature = "venus", test))]
pub(crate) const DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS: u64 = 16;

pub(crate) const DEFAULT_VIRTIO_GPU_VBLANK_HZ: u64 = 120;

pub(crate) fn virtio_gpu_vblank_interval_from_value(value: Option<&str>) -> Duration {
    let Some(value) = value else {
        return Duration::ZERO;
    };
    let hz = value
        .trim()
        .parse::<u64>()
        .unwrap_or(DEFAULT_VIRTIO_GPU_VBLANK_HZ);
    if hz == 0 {
        return Duration::ZERO;
    }
    Duration::from_nanos((1_000_000_000 / hz).max(1))
}

pub(crate) fn virtio_gpu_3d_enabled_for_pcie() -> bool {
    cfg!(feature = "venus") && env_flag("BRIDGEVM_VIRTIO_GPU_3D")
}

#[cfg(any(feature = "venus", test))]
pub(crate) fn virtio_gpu_3d_scanout_readback_interval_from_value(value: Option<&str>) -> Duration {
    let interval_ms = value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS);
    Duration::from_millis(interval_ms)
}

pub(crate) fn virtio_gpu_resolution_from_env() -> (u32, u32) {
    let value = std::env::var("BRIDGEVM_VIRTIO_GPU_RES").unwrap_or_else(|_| "1280x800".into());
    let Some((width, height)) = value.trim().split_once('x').and_then(|(width, height)| {
        Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?))
    }) else {
        panic!("BRIDGEVM_VIRTIO_GPU_RES must be WIDTHxHEIGHT, for example 1600x900");
    };
    assert!(
        width > 0 && height > 0,
        "virtio-gpu resolution must be non-zero"
    );
    (width, height)
}

pub(crate) fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let value = value.trim();
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}
