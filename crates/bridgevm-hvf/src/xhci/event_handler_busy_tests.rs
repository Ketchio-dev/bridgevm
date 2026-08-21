use super::platform_test_support::{
    enable_xhci_msix_vector0, new_platform_and_ram, program_xhci_bar0, write_event_ring_table,
    write_xhci_bar0, BarWrite, MsixVector, EVENT_RING as PLATFORM_EVENT_RING, XHCI_BAR0,
};
use super::test_support::{setup_event_ring, TestRam, EVENT_RING, TRB_SIZE};
use super::*;
use crate::machine;
use crate::msix::MsixMessage;
use crate::pcie::XHCI_MSIX_TABLE_OFFSET;
use crate::platform_virt::{MmioOp, MmioOutcome};

const EVENT_CONTROL: u32 = 32 << 10;
const MSI_ADDRESS: u64 = 0x0808_0040;
const MSI_DATA: u32 = 135;

fn setup_msix(xhci: &mut XhciController) {
    let table = u64::from(XHCI_MSIX_TABLE_OFFSET);
    xhci.mmio_write(table, 8, MSI_ADDRESS);
    xhci.mmio_write(table + 8, 4, u64::from(MSI_DATA));
    xhci.mmio_write(table + 12, 4, 0);
}

#[test]
fn busy_interrupter_defers_repeat_msi_until_software_clears_ehb() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_event_ring(&mut xhci, &mut mem);
    setup_msix(&mut xhci);

    assert!(xhci.post_event(&mut mem, 0x1111, 0, EVENT_CONTROL));
    assert_eq!(xhci.raise_pending_interrupter_msix(true, false).len(), 1);
    assert_eq!(xhci.mmio_read(0x1020, 4), 0x2);
    assert_eq!(xhci.mmio_read(0x1038, 4) & 8, 8);

    // A second event is visible and IP is set, but EHB suppresses a duplicate
    // message while software is handling the first notification.
    assert!(xhci.post_event(&mut mem, 0x2222, 0, EVENT_CONTROL));
    assert_eq!(xhci.mmio_read(0x1020, 4), 0x3);
    assert!(xhci.raise_pending_interrupter_msix(true, false).is_empty());

    // Consuming only the first event and clearing EHB re-notifies the second.
    xhci.mmio_write(0x1038, 8, (EVENT_RING + TRB_SIZE) | 8);
    assert_eq!(xhci.mmio_read(0x1038, 4) & 8, 8);
    assert_eq!(xhci.raise_pending_interrupter_msix(true, false).len(), 1);

    // Once the second event is consumed, clearing EHB produces no extra MSI.
    xhci.mmio_write(0x1038, 8, (EVENT_RING + 2 * TRB_SIZE) | 8);
    assert_eq!(xhci.mmio_read(0x1038, 4) & 8, 0);
    assert!(xhci.raise_pending_interrupter_msix(true, false).is_empty());
}

#[test]
fn erdp_ehb_renotification_reaches_the_platform_msix_queue() {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    let vector = MsixVector {
        address: machine::GIC_ITS.base + 0x40,
        data: 135,
    };
    enable_xhci_msix_vector0(&mut platform, &mut mem, vector);
    write_event_ring_table(&mut mem);
    for write in [
        BarWrite {
            offset: 0x1020,
            size: 4,
            value: 2,
        },
        BarWrite {
            offset: 0x1028,
            size: 4,
            value: 1,
        },
        BarWrite {
            offset: 0x1030,
            size: 8,
            value: super::platform_test_support::ERST,
        },
        BarWrite {
            offset: 0x1038,
            size: 8,
            value: PLATFORM_EVENT_RING,
        },
    ] {
        write_xhci_bar0(&mut platform, &mut mem, write);
    }
    assert!(platform.xhci.post_event(&mut mem, 0x1111, 0, EVENT_CONTROL));
    platform.queue_xhci_completion_msix();
    assert_eq!(platform.take_pending_msix().len(), 1);
    assert!(platform.xhci.post_event(&mut mem, 0x2222, 0, EVENT_CONTROL));
    platform.queue_xhci_completion_msix();
    assert!(platform.take_pending_msix().is_empty());

    let outcome = platform.on_mmio(
        XHCI_BAR0 + 0x1038,
        MmioOp::Write {
            size: 8,
            value: (PLATFORM_EVENT_RING + TRB_SIZE) | 8,
        },
        &mut mem,
    );
    assert_eq!(outcome, MmioOutcome::WriteAck);
    assert_eq!(
        platform.take_pending_msix(),
        vec![MsixMessage {
            vector: 0,
            address: vector.address,
            data: vector.data
        }]
    );
}
