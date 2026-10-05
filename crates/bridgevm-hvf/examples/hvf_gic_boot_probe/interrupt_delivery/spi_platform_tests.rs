//! The real VirtIO-MMIO producer queues a transition only once.
use super::*;
use bridgevm_hvf::platform_virt::FlatGuestRam;
use bridgevm_hvf::virtio_blk::INSTALLER_ISO_SLOT;
#[path = "spi_platform_fixture.rs"]
mod fixture;

#[test]
fn unchanged_level_does_not_recover_failed_real_device_spi() {
    let (mut platform, mut mem, path, slot) = fixture::queued_iso_spi();
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
    let result = complete_prepared_interrupt_delivery(ready, |_| {
        completed.set(true);
        Ok(())
    });
    std::fs::remove_file(&path).unwrap();
    assert!(
        result.is_err(),
        "failed real-device SPI was consumed with no new request and caller continued"
    );
    assert!(!completed.get());
    assert_eq!((stats.spi.drained, stats.spi.failure), (1, 1));
}
