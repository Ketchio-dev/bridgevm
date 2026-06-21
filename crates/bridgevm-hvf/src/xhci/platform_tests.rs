use super::event::USB_STS_EINT;
use super::platform_test_support::*;
use crate::platform_virt::{MmioOp, MmioOutcome};

#[test]
fn xhci_posts_enable_slot_command_completion_when_doorbell_zero_rings() {
    // Given: a guest-owned command ring with one Enable Slot TRB and an event ring.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);

    write_command_trb(
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    write_event_ring_table(&mut mem);

    let mut write_reg = |offset: u64, size: u8, value: u64| {
        assert_eq!(
            platform.on_mmio(XHCI_BAR0 + offset, MmioOp::Write { size, value }, &mut mem,),
            MmioOutcome::WriteAck
        );
    };
    write_reg(0x58, 8, COMMAND_RING | 1);
    write_reg(0x70, 8, DCBAA);
    write_reg(0x78, 4, 1);
    write_reg(0x1020, 4, 2);
    write_reg(0x1028, 4, 1);
    write_reg(0x1030, 8, ERST);
    write_reg(0x1038, 8, EVENT_RING);

    // When: software rings the host-controller command doorbell.
    write_reg(0x2000, 4, 0);

    // Then: the primary event ring receives a successful Command Completion Event.
    assert_success_completion(&mem, ENABLE_SLOT_ID);
    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + 0x1020, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(3)
    );
}

#[test]
fn xhci_posts_address_device_command_completion_when_doorbell_zero_rings() {
    // Given: a guest-owned Address Device command TRB and an event ring.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    write_command_trb(
        &mut mem,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ADDRESS_DEVICE_SLOT_ID),
    );
    write_event_ring_table(&mut mem);

    let mut write_reg = |offset: u64, size: u8, value: u64| {
        assert_eq!(
            platform.on_mmio(XHCI_BAR0 + offset, MmioOp::Write { size, value }, &mut mem,),
            MmioOutcome::WriteAck
        );
    };
    write_reg(0x58, 8, COMMAND_RING | 1);
    write_reg(0x70, 8, DCBAA);
    write_reg(0x78, 4, 1);
    write_reg(0x1020, 4, 2);
    write_reg(0x1028, 4, 1);
    write_reg(0x1030, 8, ERST);
    write_reg(0x1038, 8, EVENT_RING);

    // When: software rings BAR0 doorbell 0 at offset 0x2000.
    write_reg(0x2000, 4, 0);

    // Then: the event ring receives a successful completion for the requested slot.
    assert_success_completion(&mem, ADDRESS_DEVICE_SLOT_ID);
}

#[test]
fn xhci_erdp_ehb_write_consumes_event_interrupt_through_pci_bar() {
    // Given: a command completion event is pending through the platform BAR path.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);

    write_command_trb(
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    write_event_ring_table(&mut mem);

    {
        let mut write_reg = |offset: u64, size: u8, value: u64| {
            assert_eq!(
                platform.on_mmio(XHCI_BAR0 + offset, MmioOp::Write { size, value }, &mut mem,),
                MmioOutcome::WriteAck
            );
        };
        write_reg(0x40, 4, 1);
        write_reg(0x58, 8, COMMAND_RING | 1);
        write_reg(0x70, 8, DCBAA);
        write_reg(0x78, 4, 1);
        write_reg(0x1020, 4, 2);
        write_reg(0x1028, 4, 1);
        write_reg(0x1030, 8, ERST);
        write_reg(0x1038, 8, EVENT_RING);
        write_reg(0x2000, 4, 0);
    }

    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + 0x1020, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(3)
    );
    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + 0x44, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(u64::from(USB_STS_EINT))
    );

    // When: the guest writes ERDP with EHB set after consuming the event.
    assert_eq!(
        platform.on_mmio(
            XHCI_BAR0 + 0x1038,
            MmioOp::Write {
                size: 8,
                value: (EVENT_RING + 0x10) | 0x8,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    // Then: interrupt state clears while ERDP keeps only the dequeue pointer.
    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + 0x1020, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(2)
    );
    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + 0x44, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0)
    );
    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + 0x1038, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(EVENT_RING + 0x10)
    );
}
