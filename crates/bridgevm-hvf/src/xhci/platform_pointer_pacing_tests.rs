use super::platform_test_support::*;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::MmioOp;
use crate::xhci::{PointerInputAction, PointerPosition};
use std::time::{Duration, Instant};
const DCI5: u32 = 5;
const DCI5_RING: u64 = crate::machine::RAM_BASE + 0xa000;
const DCI5_BUFFER: u64 = crate::machine::RAM_BASE + 0xb000;
fn emitted(platform: &crate::platform_virt::VirtPlatform) -> (u64, u64, u64) {
    let s = platform.xhci_pointer_input_report_stats();
    (
        s.emitted_move_reports,
        s.emitted_button_reports,
        s.emitted_release_reports,
    )
}

fn write_u32(mem: &mut crate::platform_virt::FlatGuestRam, gpa: u64, value: u32) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn write_u64(mem: &mut crate::platform_virt::FlatGuestRam, gpa: u64, value: u64) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn configure_dci5(
    platform: &mut crate::platform_virt::VirtPlatform,
    mem: &mut crate::platform_virt::FlatGuestRam,
) {
    write_event_ring_table(mem);
    write_u64(mem, DCBAA + 8, OUTPUT_CONTEXT);
    write_u32(
        mem,
        INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        1 << DCI5,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + 0xc0 + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    write_u64(
        mem,
        INPUT_CONTEXT + 0xc0 + EP_TR_DEQUEUE_OFFSET,
        DCI5_RING | TRB_CYCLE,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + 0xc0 + EP_CONTEXT_DWORD4_OFFSET,
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
    for index in 0..3 {
        let trb = DCI5_RING + TRB_SIZE * index;
        write_u64(mem, trb, DCI5_BUFFER + 0x20 * index);
        write_u32(mem, trb + 8, 6);
        write_u32(mem, trb + 12, (TRB_TYPE_NORMAL << 10) | 1);
    }
}

#[test]
fn erdp_mmio_does_not_bypass_pointer_report_pacing() {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci5(&mut platform, &mut mem);
    platform.set_xhci_report_interval(Duration::from_millis(30));
    let base = Instant::now();
    platform.set_host_now(base);
    let position = PointerPosition::new(1_024, 2_048).unwrap();
    platform
        .queue_xhci_pointer_input_actions_with_mem(
            &[
                PointerInputAction::Move(position),
                PointerInputAction::Click(position),
            ],
            &mut mem,
        )
        .unwrap();
    assert_eq!(emitted(&platform), (0, 1, 0));
    // A guest DCI5 doorbell cannot bypass the platform conjunction either.
    platform.on_mmio(
        XHCI_BAR0 + 0x2004,
        MmioOp::Write {
            size: 4,
            value: u64::from(DCI5),
        },
        &mut mem,
    );
    assert_eq!(emitted(&platform), (0, 1, 0));
    // Time alone is insufficient until the guest consumes button's event.
    platform.set_host_now(base + Duration::from_millis(30));
    platform.drain_xhci_pointer_input_reports(&mut mem);
    assert_eq!(emitted(&platform), (0, 1, 0));
    platform.on_mmio(
        XHCI_BAR0 + 0x1038,
        MmioOp::Write {
            size: 8,
            value: (EVENT_RING + TRB_SIZE * 2) | 8,
        },
        &mut mem,
    );
    assert_eq!(emitted(&platform), (0, 1, 1));
}
