//! Existing real-device setup, shared with the delivery regression.
use super::*;

pub(super) fn queued_iso_spi() -> (VirtPlatform, FlatGuestRam, std::path::PathBuf, u64) {
    let path =
        std::env::temp_dir().join(format!("bridgevm-spi-delivery-test-{}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    drop(file);
    std::fs::write(&path, vec![0u8; 1024]).unwrap();
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    platform.attach_virtio_iso(&path).unwrap();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
    let slot = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT).base;
    let table = machine::RAM_BASE + 0x1000;
    let avail = table + 8 * 16;
    let header = machine::RAM_BASE + 0x4000;
    let data = machine::RAM_BASE + 0x5000;
    let status = machine::RAM_BASE + 0x6000;
    for (index, address, size, flags, next) in [
        (0, header, 16u32, 1u16, 1u16),
        (1, data, 512, 3, 2),
        (2, status, 1, 2, 0),
    ] {
        let descriptor = table + index * 16;
        assert!(mem.write_bytes(descriptor, &address.to_le_bytes()));
        assert!(mem.write_bytes(descriptor + 8, &size.to_le_bytes()));
        assert!(mem.write_bytes(descriptor + 12, &flags.to_le_bytes()));
        assert!(mem.write_bytes(descriptor + 14, &next.to_le_bytes()));
    }
    assert!(mem.write_bytes(header, &0u32.to_le_bytes()));
    assert!(mem.write_bytes(header + 8, &1u64.to_le_bytes()));
    assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
    assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));
    for (register, value) in [
        (0x38, 8),
        (0x28, 4096),
        (0x3c, 4096),
        (0x40, table >> 12),
        (0x50, 0),
    ] {
        assert_eq!(
            platform.on_mmio(slot + register, MmioOp::Write { size: 4, value }, &mut mem),
            MmioOutcome::WriteAck
        );
    }
    assert_eq!(mem.read_bytes(status, 1).unwrap(), [0]);
    (platform, mem, path, slot)
}
