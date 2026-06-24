use crate::fwcfg::GuestMemoryMut;

use super::{
    setup_input_report::SetupInputReport,
    setup_input_report::{HID_BOOT_KEYBOARD_NO_KEY_REPORT, HID_BOOT_KEYBOARD_REPORT_LEN},
    XhciController,
};

const TRB_SIZE: usize = 16;
const TRB_SIZE_BYTES: u64 = 16;
const TRB_CYCLE: u32 = 1;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_NORMAL: u32 = 1;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TRANSFER_LENGTH_MASK: u32 = 0x1f_ffff;
const LINK_TRB_POINTER_MASK: u64 = !0xf;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const COMPLETION_CODE_SHIFT: u32 = 24;
const SLOT_ID: u32 = 1;
const ENDPOINT_ID_DCI3: u32 = 3;
const EVENT_ENDPOINT_ID_SHIFT: u32 = 16;
const EVENT_SLOT_ID_SHIFT: u32 = 24;
const MAX_LINK_TRBS_PER_DOORBELL: usize = 8;

struct InterruptTransferTrb {
    gpa: u64,
    parameter: u64,
    status: u32,
    control: u32,
}

impl XhciController {
    pub(super) fn process_dci3_interrupt_in_transfer(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        for _ in 0..MAX_LINK_TRBS_PER_DOORBELL {
            let transfer_ring = self.slot1_dci3_dequeue;
            if transfer_ring == 0 {
                return false;
            }
            let Some(interrupt_transfer) = read_transfer_trb(mem, transfer_ring) else {
                return false;
            };
            let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
            if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
                return false;
            }
            match trb_type(interrupt_transfer.control) {
                TRB_TYPE_LINK => {
                    self.slot1_dci3_dequeue = interrupt_transfer.parameter & LINK_TRB_POINTER_MASK;
                    if interrupt_transfer.control & TRB_LINK_TOGGLE_CYCLE != 0 {
                        self.slot1_dci3_dcs = !self.slot1_dci3_dcs;
                    }
                    self.write_slot1_dci3_output_dequeue(mem);
                }
                TRB_TYPE_NORMAL => {
                    let Some(next_dequeue) = interrupt_transfer.gpa.checked_add(TRB_SIZE_BYTES)
                    else {
                        return false;
                    };
                    let queued_report = self.boot_keyboard_report_queue.peek();
                    let report = queued_report
                        .map(SetupInputReport::bytes)
                        .unwrap_or(HID_BOOT_KEYBOARD_NO_KEY_REPORT);
                    let transfer_length = trb_transfer_length(interrupt_transfer.status);
                    let write_len = transfer_length.min(HID_BOOT_KEYBOARD_REPORT_LEN);
                    let Ok(write_len) = usize::try_from(write_len) else {
                        return false;
                    };
                    if write_len > 0
                        && !mem.write_bytes(interrupt_transfer.parameter, &report[..write_len])
                    {
                        return false;
                    }
                    let residual_length =
                        transfer_length.saturating_sub(HID_BOOT_KEYBOARD_REPORT_LEN);
                    let event_status =
                        (COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT) | residual_length;
                    let event_control = transfer_event_control(SLOT_ID, ENDPOINT_ID_DCI3);
                    let posted =
                        self.post_event(mem, interrupt_transfer.gpa, event_status, event_control);
                    if posted {
                        if write_len > 0 {
                            if let Some(queued_report) = queued_report {
                                self.record_setup_input_report_emitted(
                                    queued_report,
                                    report,
                                    interrupt_transfer.gpa,
                                    interrupt_transfer.parameter,
                                );
                                self.boot_keyboard_report_queue.pop_front();
                            }
                        }
                        self.slot1_dci3_dequeue = next_dequeue;
                        self.write_slot1_dci3_output_dequeue(mem);
                    }
                    return posted;
                }
                _ => return false,
            }
        }
        false
    }
}

fn read_transfer_trb(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<InterruptTransferTrb> {
    let raw = mem.read_bytes(gpa, TRB_SIZE)?;
    Some(InterruptTransferTrb {
        gpa,
        parameter: read_u64(&raw, 0)?,
        status: read_u32(&raw, 8)?,
        control: read_u32(&raw, 12)?,
    })
}

fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
}

fn trb_transfer_length(status: u32) -> u32 {
    status & TRB_TRANSFER_LENGTH_MASK
}

fn transfer_event_control(slot_id: u32, endpoint_id: u32) -> u32 {
    (slot_id << EVENT_SLOT_ID_SHIFT)
        | (endpoint_id << EVENT_ENDPOINT_ID_SHIFT)
        | (TRB_TYPE_TRANSFER_EVENT << TRB_TYPE_SHIFT)
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    let array: [u8; 4] = raw.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    let array: [u8; 8] = raw.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}
