//! PCI ISR read-to-clear; core virtio-MMIO continues to require explicit ACK.
#[path = "virtio_blk_isr/support.rs"]
mod support;
use support::*;

#[test]
fn legacy_pio_isr_read_deasserts_completion_and_rearms() {
    read_ack(Transport::Pio, &[1]);
}

#[test]
fn pci_bar_isr_read_deasserts_completion_and_rearms() {
    read_ack(Transport::Bar, &[1, 2, 4, 8]);
}

fn read_ack(transport: Transport, widths: &[u8]) {
    for &size in widths {
        let mut f = Fixture::new(transport);
        for slot in 0..2 {
            f.submit(slot);
            for (off, width) in [
                (BAR + 0x1001, 1),
                (BAR + 0x1002, 2),
                (BAR + 0x1004, 4),
                (PIO + 0x12, 1),
            ] {
                f.read(off, width);
                assert!(f.platform.take_pending_spi_levels().is_empty());
            }
            assert_eq!(f.read(f.isr(), size), 1);
            assert_eq!(f.platform.take_pending_spi_levels(), [(IRQ, false)]);
            assert_eq!(f.read(f.isr(), size), 0);
            assert!(f.platform.take_pending_spi_levels().is_empty());
        }
    }
}

#[test]
fn explicit_pci_ack_compatibility_is_preserved() {
    for transport in [Transport::Pio, Transport::Bar] {
        let mut f = Fixture::new(transport);
        f.submit(0);
        f.write(f.isr(), 1, 0);
        f.write(f.isr(), 1, 2);
        assert!(f.platform.take_pending_spi_levels().is_empty());
        f.write(f.isr(), 1, 1);
        assert_eq!(f.platform.take_pending_spi_levels(), [(IRQ, false)]);
        assert_eq!(f.read(f.isr(), 1), 0);
        f.submit(1);
    }
}

#[test]
fn pci_reset_deasserts_an_unread_completion() {
    for transport in [Transport::Pio, Transport::Bar] {
        let mut f = Fixture::new(transport);
        f.submit(0);
        let status = match transport {
            Transport::Pio => PIO + 0x12,
            Transport::Bar => BAR + 0x70,
        };
        f.write(status, 1, 0);
        assert_eq!(f.platform.take_pending_spi_levels(), [(IRQ, false)]);
        assert_eq!(f.read(f.isr(), 1), 0);
    }
}

#[test]
fn isr_ack_does_not_reprogram_msix_table_or_pba() {
    for transport in [Transport::Pio, Transport::Bar] {
        let mut f = Fixture::new(transport);
        f.write(MSIX, 8, 0xfee0_0000);
        f.write(MSIX + 8, 4, 0x51);
        f.write(MSIX + 12, 4, 1);
        f.submit(0);
        assert_eq!(f.read(f.isr(), 1), 1);
        assert_eq!(f.read(MSIX, 8), 0xfee0_0000);
        assert_eq!(f.read(MSIX + 8, 4), 0x51);
        assert_eq!(f.read(MSIX + 12, 4), 1);
        f.write(MSIX + 0x800, 8, u64::MAX);
        assert_eq!(f.read(MSIX + 0x800, 8), 0);
        assert!(f.platform.take_pending_msix().is_empty());
        assert_eq!(f.platform.take_pending_spi_levels(), [(IRQ, false)]);
    }
}

#[test]
fn bar_common_interrupt_status_alias_still_requires_ack() {
    let mut f = Fixture::new(Transport::Bar);
    f.submit(0);
    for _ in 0..2 {
        assert_eq!(f.read(BAR + 0x60, 4), 1);
        assert!(f.platform.take_pending_spi_levels().is_empty());
    }
    f.write(BAR + 0x64, 4, 1);
    assert_eq!(f.platform.take_pending_spi_levels(), [(IRQ, false)]);
    assert_eq!(f.read(BAR + 0x60, 4), 0);
}

#[test]
fn native_mmio_interrupt_status_still_requires_explicit_ack() {
    use bridgevm_hvf::{
        fwcfg::GuestMemoryMut,
        virtio_blk::{VirtioMmioBlock, VirtioMmioBlockResult},
    };
    let media = Media::new();
    let mut dev = VirtioMmioBlock::open_read_only(media.path()).unwrap();
    let mut mem = memory();
    for (off, value) in [
        (0x70, 0),
        (0x70, 3),
        (0x28, 4096),
        (0x30, 0),
        (0x38, 128),
        (0x3c, 4096),
        (0x40, DESC >> 12),
        (0x70, 7),
    ] {
        assert_eq!(
            dev.access(off, true, 4, value, &mut mem),
            VirtioMmioBlockResult::WriteAck
        );
    }
    assert!(mem.write_bytes(AVAIL + 2, &1u16.to_le_bytes()));
    dev.access(0x50, true, 4, 0, &mut mem);
    assert_eq!(dev.stats().read_count, 1);
    assert_eq!(mem.read_bytes(USED + 2, 2).unwrap(), [1, 0]);
    for _ in 0..2 {
        assert_eq!(
            dev.access(0x60, false, 4, 0, &mut mem),
            VirtioMmioBlockResult::ReadValue(1)
        );
        assert!(dev.interrupt_line_level());
    }
    for value in [0, 2] {
        dev.access(0x64, true, 4, value, &mut mem);
        assert!(dev.interrupt_line_level());
    }
    dev.access(0x64, true, 4, 1, &mut mem);
    assert!(!dev.interrupt_line_level());
    assert_eq!(
        dev.access(0x60, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(0)
    );
}
