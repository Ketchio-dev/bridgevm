//! xHCI interrupter MSI-X semantics: IMAN.IP is auto-cleared when the MSI-X
//! message is sent (xHCI 4.17.5), so one posted event yields exactly one
//! message and an un-acknowledged interrupter does not re-fire on every flush.

use super::test_support::{setup_event_ring, TestRam};
use super::*;
use crate::msix::MsixMessage;
use crate::pcie::XHCI_MSIX_TABLE_OFFSET;

const MSI_ADDRESS: u64 = 0x0808_0040;
const MSI_DATA: u32 = 130;
const IMAN_INTERRUPTER0: u64 = 0x1020;
const TRANSFER_EVENT_CONTROL: u32 = 32 << 10;

fn program_vector0(xhci: &mut XhciController) {
    let base = u64::from(XHCI_MSIX_TABLE_OFFSET);
    xhci.mmio_write(base, 8, MSI_ADDRESS);
    xhci.mmio_write(base + 8, 4, u64::from(MSI_DATA));
}

fn unmask_vector0(xhci: &mut XhciController) {
    xhci.mmio_write(u64::from(XHCI_MSIX_TABLE_OFFSET) + 12, 4, 0);
}

fn mask_vector0(xhci: &mut XhciController) {
    xhci.mmio_write(u64::from(XHCI_MSIX_TABLE_OFFSET) + 12, 4, 1);
}

fn expected_message() -> MsixMessage {
    MsixMessage {
        vector: 0,
        address: MSI_ADDRESS,
        data: MSI_DATA,
    }
}

#[test]
fn one_event_raises_exactly_one_message_and_auto_clears_ip() {
    // Given: interrupter 0 is enabled, MSI-X vector 0 is programmed and unmasked,
    // and one event has been posted (IMAN.IP set).
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_event_ring(&mut xhci, &mut mem);
    program_vector0(&mut xhci);
    unmask_vector0(&mut xhci);
    assert!(xhci.post_event(&mut mem, 0x1111, 0, TRANSFER_EVENT_CONTROL));
    assert_eq!(xhci.mmio_read(IMAN_INTERRUPTER0, 4), 0x3);

    // When: the interrupter is flushed once.
    let first = xhci.raise_pending_interrupter_msix(true, false);

    // Then: exactly one message is delivered and IMAN.IP is auto-cleared, so a
    // second flush without a new event delivers nothing.
    assert_eq!(first, vec![expected_message()]);
    assert_eq!(xhci.mmio_read(IMAN_INTERRUPTER0, 4), 0x2);
    assert_eq!(xhci.raise_pending_interrupter_msix(true, false), Vec::new());
}

#[test]
fn masked_vector_keeps_ip_pending_until_drained_after_unmask() {
    // Given: MSI-X vector 0 is programmed but left masked when the event posts.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_event_ring(&mut xhci, &mut mem);
    program_vector0(&mut xhci);
    mask_vector0(&mut xhci);
    assert!(xhci.post_event(&mut mem, 0x2222, 0, TRANSFER_EVENT_CONTROL));

    // When: the interrupter is flushed while masked.
    let masked = xhci.raise_pending_interrupter_msix(true, false);

    // Then: no message is sent, the PBA records it, and IMAN.IP stays set.
    assert_eq!(masked, Vec::new());
    assert_eq!(
        xhci.mmio_read(u64::from(crate::pcie::XHCI_MSIX_PBA_OFFSET), 8),
        1
    );
    assert_eq!(xhci.mmio_read(IMAN_INTERRUPTER0, 4), 0x3);

    // When: software unmasks the vector and the pending message is drained.
    unmask_vector0(&mut xhci);
    let drained = xhci.drain_pending_msix(true, false);

    // Then: the deferred message is delivered and IMAN.IP is finally cleared.
    assert_eq!(drained, vec![expected_message()]);
    assert_eq!(xhci.mmio_read(IMAN_INTERRUPTER0, 4), 0x2);
    assert_eq!(xhci.drain_pending_msix(true, false), Vec::new());
}

#[test]
fn guest_iman_write_one_to_clear_stops_further_messages_without_new_event() {
    // Given: an unmasked interrupter with one posted event.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_event_ring(&mut xhci, &mut mem);
    program_vector0(&mut xhci);
    unmask_vector0(&mut xhci);
    assert!(xhci.post_event(&mut mem, 0x3333, 0, TRANSFER_EVENT_CONTROL));

    // When: the guest clears IMAN.IP (RW1C) itself before any flush.
    xhci.mmio_write(IMAN_INTERRUPTER0, 4, 0x3);
    assert_eq!(xhci.mmio_read(IMAN_INTERRUPTER0, 4), 0x2);

    // Then: no message is raised until a new event posts.
    assert_eq!(xhci.raise_pending_interrupter_msix(true, false), Vec::new());
}

#[test]
fn two_events_each_raise_exactly_one_message() {
    // Given: an unmasked, enabled interrupter.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_event_ring(&mut xhci, &mut mem);
    program_vector0(&mut xhci);
    unmask_vector0(&mut xhci);

    // When: two events are posted back-to-back, each followed by a flush.
    assert!(xhci.post_event(&mut mem, 0x4444, 0, TRANSFER_EVENT_CONTROL));
    let first = xhci.raise_pending_interrupter_msix(true, false);
    assert!(xhci.post_event(&mut mem, 0x5555, 0, TRANSFER_EVENT_CONTROL));
    let second = xhci.raise_pending_interrupter_msix(true, false);

    // Then: exactly one message per event and no re-raise storm.
    assert_eq!(first, vec![expected_message()]);
    assert_eq!(second, vec![expected_message()]);
    assert_eq!(xhci.raise_pending_interrupter_msix(true, false), Vec::new());
}
