//! The real VirtIO-MMIO producer queues a transition only once.
use super::*;
use bridgevm_hvf::platform_virt::FlatGuestRam;
use bridgevm_hvf::virtio_blk::INSTALLER_ISO_SLOT;

#[test]
fn unchanged_level_does_not_recover_failed_real_device_spi() {
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
    let mut pending = Vec::new();
    platform.drain_pending_spi_levels_into(&mut pending);
    let intid = machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32));
    assert_eq!(pending, vec![(intid, true)]);
    let delivered = deliver_spi_levels(&mut pending, |_, _| -345);
    assert_eq!(
        platform.on_mmio(slot + 0x60, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(1)
    );
    assert!(
        platform.take_pending_spi_levels().is_empty(),
        "unchanged level unexpectedly regenerated a request"
    );
    let mut stats = RunLoopDrainStats::new(false);
    let ready = stats.pending_spi_delivery(
        DrainContext {
            location: DrainLocation::DataAbort,
            exit: 7,
            pc: 0x1234,
        },
        delivered,
    );
    let completed = std::cell::Cell::new(false);
    let result = complete_prepared_spi_delivery(ready, |_| completed.set(true));
    std::fs::remove_file(&path).unwrap();
    assert!(
        result.is_err(),
        "failed real-device SPI was consumed with no new request and caller continued"
    );
    assert!(!completed.get());
    assert_eq!((stats.spi.drained, stats.spi.failure), (1, 1));
}
