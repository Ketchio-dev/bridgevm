use super::super::*;
use super::helpers::*;
use std::time::{Duration, Instant};

const REFRESH_HZ: u64 = PCI_DEVICE_CFG_OFFSET + 20;
const RETRACE_ENABLE: u64 = PCI_DEVICE_CFG_OFFSET + 24;
const RETRACE_EVENT: u64 = 1 << 31;

#[test]
fn dod_refresh_uses_config_interrupts_at_the_selected_rate_without_3d() {
    let mut dev = VirtioPciGpu::new(1920, 1080);
    let mut mem = TestMem::new(0x4000_0000, 4096);
    program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x40);
    program_config_msix_vector(&mut dev, 0);
    assert_eq!(
        pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 16, 4, &mut mem),
        u64::from(u32::from_le_bytes(*b"BVR1"))
    );
    for hz in [
        30, 60, 75, 90, 120, 144, 165, 180, 240, 360, 480, 540, 600, 720,
    ] {
        pci_write(&mut dev, REFRESH_HZ, 4, hz, &mut mem);
        pci_write(&mut dev, RETRACE_ENABLE, 4, 1, &mut mem);
        let now = Instant::now();
        let period = Duration::from_nanos(1_000_000_000 / hz);
        dev.gpu.drain_host_vblank_at(&mut mem, now);
        assert_eq!(
            pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET, 4, &mut mem),
            RETRACE_EVENT
        );
        assert_eq!(dev.raise_pending_msix(true, false).len(), 1);
        pci_write(
            &mut dev,
            PCI_DEVICE_CFG_OFFSET + 4,
            4,
            RETRACE_EVENT,
            &mut mem,
        );
        dev.gpu
            .drain_host_vblank_at(&mut mem, now + period - Duration::from_nanos(1));
        assert_eq!(pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET, 4, &mut mem), 0);
        assert!(dev.raise_pending_msix(true, false).is_empty());
        dev.gpu.drain_host_vblank_at(&mut mem, now + period);
        assert_eq!(
            pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET, 4, &mut mem),
            RETRACE_EVENT
        );
        assert_eq!(dev.raise_pending_msix(true, false).len(), 1);
        pci_write(&mut dev, RETRACE_ENABLE, 4, 0, &mut mem);
        dev.gpu.drain_host_vblank_at(&mut mem, now + period * 10);
        assert_eq!(pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET, 4, &mut mem), 0);
    }
}

#[test]
fn dod_refresh_rejects_unbounded_guest_timing_and_reset_disarms_it() {
    let mut dev = VirtioPciGpu::new(16, 16);
    let mut mem = TestMem::new(0, 4096);
    pci_write(&mut dev, REFRESH_HZ, 4, 720, &mut mem);
    for hz in [0, 29, 721, u64::MAX] {
        pci_write(&mut dev, REFRESH_HZ, 4, hz, &mut mem);
        assert_eq!(pci_read(&mut dev, REFRESH_HZ, 4, &mut mem), 720);
    }
    pci_write(&mut dev, RETRACE_ENABLE, 4, 1, &mut mem);
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0, &mut mem);
    dev.gpu.drain_host_vblank_at(&mut mem, Instant::now());
    assert_eq!(pci_read(&mut dev, RETRACE_ENABLE, 4, &mut mem), 0);
    assert_eq!(pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET, 4, &mut mem), 0);
    assert!(!dev.interrupt_line_level());
}
