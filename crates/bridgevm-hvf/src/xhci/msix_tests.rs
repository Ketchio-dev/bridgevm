use super::platform_test_support::*;
use crate::machine;
use crate::msix::MsixMessage;

#[test]
fn xhci_enable_slot_completion_queues_msix_message_when_vector_unmasked() {
    // Given: xHCI BAR0, vector 0 MSI-X, command ring, and interrupter 0 are enabled.
    const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x40;
    const MSI_DATA: u32 = 0x83;
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    enable_xhci_msix_vector0(
        &mut platform,
        &mut mem,
        MsixVector {
            address: MSI_ADDRESS,
            data: MSI_DATA,
        },
    );
    write_command_trb(
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    write_event_ring_table(&mut mem);

    for write in [
        BarWrite {
            offset: 0x40,
            size: 4,
            value: 1,
        },
        BarWrite {
            offset: 0x58,
            size: 8,
            value: COMMAND_RING | 1,
        },
        BarWrite {
            offset: 0x70,
            size: 8,
            value: DCBAA,
        },
        BarWrite {
            offset: 0x78,
            size: 4,
            value: 1,
        },
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
            value: ERST,
        },
        BarWrite {
            offset: 0x1038,
            size: 8,
            value: EVENT_RING,
        },
    ] {
        write_xhci_bar0(&mut platform, &mut mem, write);
    }

    // When: software rings the host-controller command doorbell.
    write_xhci_bar0(
        &mut platform,
        &mut mem,
        BarWrite {
            offset: 0x2000,
            size: 4,
            value: 0,
        },
    );

    // Then: completion reaches both the guest event ring and the platform MSI-X queue.
    assert_success_completion(&mem, ENABLE_SLOT_ID);
    assert_eq!(
        platform.take_pending_msix(),
        vec![MsixMessage {
            vector: 0,
            address: MSI_ADDRESS,
            data: MSI_DATA,
        }]
    );
}

#[test]
fn xhci_ep0_transfer_completion_queues_msix_message_when_vector_unmasked() {
    // Given: slot 1 EP0 has a GET_DESCRIPTOR transfer and xHCI MSI-X vector 0 is unmasked.
    const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x40;
    const MSI_DATA: u32 = 0x83;
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    enable_xhci_msix_vector0(
        &mut platform,
        &mut mem,
        MsixVector {
            address: MSI_ADDRESS,
            data: MSI_DATA,
        },
    );
    write_command_trb_with_parameter(
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_event_ring_table(&mut mem);
    write_ep0_input_context(&mut mem, EP0_RING | 1);
    write_get_descriptor_device_transfer(&mut mem);

    for write in [
        BarWrite {
            offset: 0x40,
            size: 4,
            value: 1,
        },
        BarWrite {
            offset: 0x58,
            size: 8,
            value: COMMAND_RING | 1,
        },
        BarWrite {
            offset: 0x70,
            size: 8,
            value: DCBAA,
        },
        BarWrite {
            offset: 0x78,
            size: 4,
            value: 1,
        },
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
            value: ERST,
        },
        BarWrite {
            offset: 0x1038,
            size: 8,
            value: EVENT_RING,
        },
    ] {
        write_xhci_bar0(&mut platform, &mut mem, write);
    }
    write_xhci_bar0(
        &mut platform,
        &mut mem,
        BarWrite {
            offset: 0x2000,
            size: 4,
            value: 0,
        },
    );
    assert_eq!(platform.take_pending_msix().len(), 1);

    // When: software rings slot 1 endpoint 0 at doorbell[1].
    write_xhci_bar0(
        &mut platform,
        &mut mem,
        BarWrite {
            offset: 0x2004,
            size: 4,
            value: 1,
        },
    );

    // Then: the EP0 completion reaches the event ring and queues vector 0 again.
    assert_eq!(
        read_bytes(&mem, DATA_STAGE_BUFFER, DEVICE_DESCRIPTOR.len()),
        DEVICE_DESCRIPTOR
    );
    assert_success_transfer_event(&mem, EVENT_RING + 0x10);
    assert_eq!(
        platform.take_pending_msix(),
        vec![MsixMessage {
            vector: 0,
            address: MSI_ADDRESS,
            data: MSI_DATA,
        }]
    );
}
