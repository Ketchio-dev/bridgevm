use super::super::*;
use super::helpers::*;
use std::time::{Duration, Instant};

#[test]
fn dod_refresh_uses_config_interrupts_at_the_selected_rate_without_3d() {
    let mut dev = VirtioPciGpu::new(1920, 1080);
    let mut mem = TestMem::new(0x4000_0000, 4096);
    program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x40);
    dev.gpu.config_msix_vector = 0;
    assert_eq!(
        dev.gpu.config_read(16, 4),
        u64::from(u32::from_le_bytes(*b"BVR1"))
    );
    for hz in [
        30, 60, 75, 90, 120, 144, 165, 180, 240, 360, 480, 540, 600, 720,
    ] {
        dev.gpu.config_write(20, 4, hz);
        dev.gpu.config_write(24, 4, 1);
        let now = Instant::now();
        let period = Duration::from_nanos(1_000_000_000 / hz);
        dev.gpu.drain_host_vblank_at(&mut mem, now);
        assert_eq!(dev.gpu.config_read(0, 4), 1 << 31);
        assert_eq!(dev.raise_pending_msix(true, false).len(), 1);
        dev.gpu.config_write(4, 4, 1 << 31);
        dev.gpu
            .drain_host_vblank_at(&mut mem, now + period - Duration::from_nanos(1));
        assert_eq!(dev.gpu.config_read(0, 4), 0);
        assert!(dev.raise_pending_msix(true, false).is_empty());
        dev.gpu.drain_host_vblank_at(&mut mem, now + period);
        assert_eq!(dev.gpu.config_read(0, 4), 1 << 31);
        assert_eq!(dev.raise_pending_msix(true, false).len(), 1);
        dev.gpu.config_write(4, 4, 1 << 31);
        dev.gpu.config_write(24, 4, 0);
        dev.gpu.drain_host_vblank_at(&mut mem, now + period * 10);
        assert_eq!(dev.gpu.config_read(0, 4), 0);
    }
}

#[test]
fn dod_refresh_rejects_unbounded_guest_timing_and_reset_disarms_it() {
    let mut dev = VirtioPciGpu::new(16, 16);
    dev.gpu.config_write(20, 4, 720);
    for hz in [0, 29, 721, u64::MAX] {
        dev.gpu.config_write(20, 4, hz);
        assert_eq!(dev.gpu.config_read(20, 4), 720);
    }
    dev.gpu.config_write(24, 4, 1);
    dev.gpu.reset_runtime_state();
    let mut mem = TestMem::new(0, 4096);
    dev.gpu.drain_host_vblank_at(&mut mem, Instant::now());
    assert_eq!(dev.gpu.config_read(24, 4), 0);
    assert_eq!(dev.gpu.config_read(0, 4), 0);
    assert!(!dev.gpu.pending_config_change);
}
