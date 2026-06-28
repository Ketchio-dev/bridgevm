use super::platform_test_support::*;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::{FlatGuestRam, VirtPlatform};

pub(super) fn configure_dci3_interrupt_in_over_bar0(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
) {
    write_event_ring_table(mem);
    write_u64(mem, DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    write_u32(
        mem,
        INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    write_u64(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        DCI3_RING | TRB_CYCLE,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
    write_command_trb_with_parameter(
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    for (offset, size, value) in [
        (0x58, 8, COMMAND_RING | TRB_CYCLE),
        (0x70, 8, DCBAA),
        (0x78, 4, 1),
        (0x1020, 4, 2),
        (0x1028, 4, 1),
        (0x1030, 8, ERST),
        (0x1038, 8, EVENT_RING),
        (0x2000, 4, 0),
    ] {
        write_xhci_bar0(
            platform,
            mem,
            BarWrite {
                offset,
                size,
                value,
            },
        );
    }
    assert_success_completion(mem, ENABLE_SLOT_ID);
}

pub(super) fn write_dci3_normal_trb(mem: &mut FlatGuestRam, trb_gpa: u64, buffer_gpa: u64) {
    write_u64(mem, trb_gpa, buffer_gpa);
    write_u32(mem, trb_gpa + 8, 8);
    write_u32(mem, trb_gpa + 12, (TRB_TYPE_NORMAL << 10) | 1);
}

pub(super) fn ring_dci3_doorbell(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    write_xhci_bar0(
        platform,
        mem,
        BarWrite {
            offset: 0x2004,
            size: 4,
            value: u64::from(DCI3),
        },
    );
}

pub(super) fn acknowledge_event_ring_dequeue(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    event_index: u64,
) {
    let erdp = (EVENT_RING + (TRB_SIZE * event_index)) | 0x8;
    write_xhci_bar0(
        platform,
        mem,
        BarWrite {
            offset: 0x1038,
            size: 4,
            value: erdp,
        },
    );
}

pub(super) fn assert_success_dci3_transfer_event_for_trb(
    mem: &FlatGuestRam,
    event_gpa: u64,
    trb_gpa: u64,
) {
    assert_eq!(read_u64(mem, event_gpa), trb_gpa);
    assert_eq!(read_u32(mem, event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = read_u32(mem, event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, DCI3);
    assert_eq!(control >> 24, ENABLE_SLOT_ID);
    assert_eq!(control & 1, 1);
}

fn write_u32(mem: &mut FlatGuestRam, gpa: u64, value: u32) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn write_u64(mem: &mut FlatGuestRam, gpa: u64, value: u64) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}
