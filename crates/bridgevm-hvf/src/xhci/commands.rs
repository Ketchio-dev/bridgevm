use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

const DOORBELL0: u64 = 0x2000;
const TRB_SIZE: usize = 16;
const TRB_SIZE_BYTES: u64 = 16;
const COMMAND_RING_POINTER_MASK: u64 = !0x3f;
const LINK_TRB_POINTER_MASK: u64 = !0xf;
const TRB_CYCLE: u32 = 1;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_DISABLE_SLOT: u32 = 10;
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
const TRB_TYPE_EVALUATE_CONTEXT: u32 = 13;
const TRB_TYPE_STOP_ENDPOINT: u32 = 15;
const TRB_TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const TRB_TYPE_RESET_DEVICE: u32 = 17;
const TRB_TYPE_NO_OP_COMMAND: u32 = 23;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const ADDRESS_DEVICE_BSR: u32 = 1 << 9;
const SLOT_ID: u32 = 1;
const COMMAND_SLOT_ID_SHIFT: u32 = 24;
const COMMAND_SLOT_ID_MASK: u32 = 0xff;
const COMMAND_ENDPOINT_ID_SHIFT: u32 = 16;
const COMMAND_ENDPOINT_ID_MASK: u32 = 0x1f;
const ENDPOINT_ID_EP0: u32 = 1;
const ENDPOINT_ID_DCI3: u32 = 3;
const ENDPOINT_ID_DCI5: u32 = 5;
const TR_DEQUEUE_POINTER_MASK: u64 = !0xf;
const MAX_LINK_TRBS_PER_DOORBELL: usize = 8;

pub(super) const fn is_command_doorbell(offset: u64, size: u8) -> bool {
    offset == DOORBELL0 && size == 4
}

impl XhciController {
    pub(super) fn write_crcr_low(&mut self, value: u32) {
        self.crcr = (self.crcr & !0xffff_ffff) | u64::from(value);
        self.sync_command_ring_from_crcr();
    }

    pub(super) fn write_crcr_high(&mut self, value: u32) {
        self.crcr = (self.crcr & 0xffff_ffff) | (u64::from(value) << 32);
        self.sync_command_ring_from_crcr();
    }

    pub(super) fn process_command_doorbell(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        for _ in 0..MAX_LINK_TRBS_PER_DOORBELL {
            let command_trb = self.command_dequeue;
            if command_trb == 0 {
                return false;
            }
            let mut raw_command = [0u8; TRB_SIZE];
            if !mem.read_into(command_trb, &mut raw_command) {
                return false;
            }
            let Some(command_control) = read_u32(&raw_command, 12) else {
                return false;
            };
            let expected_cycle = if self.command_cycle { TRB_CYCLE } else { 0 };
            if command_control & TRB_CYCLE != expected_cycle {
                return false;
            }
            match trb_type(command_control) {
                TRB_TYPE_LINK => {
                    let Some(link_target) = read_u64(&raw_command, 0) else {
                        return false;
                    };
                    self.command_dequeue = link_target & LINK_TRB_POINTER_MASK;
                    if command_control & TRB_LINK_TOGGLE_CYCLE != 0 {
                        self.command_cycle = !self.command_cycle;
                    }
                }
                TRB_TYPE_ENABLE_SLOT => {
                    let posted = self.post_command_completion(mem, command_trb, SLOT_ID);
                    if posted {
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_DISABLE_SLOT => {
                    let slot_id = command_slot_id(command_control);
                    let posted = self.post_command_completion(mem, command_trb, slot_id);
                    if posted {
                        if slot_id == SLOT_ID {
                            self.usb_configuration = 0;
                            self.disable_slot1_context(mem);
                        }
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_STOP_ENDPOINT => {
                    let slot_id = command_slot_id(command_control);
                    let endpoint_id = command_endpoint_id(command_control);
                    let posted = self.post_command_completion(mem, command_trb, slot_id);
                    if posted {
                        if slot_id == SLOT_ID && endpoint_id == ENDPOINT_ID_EP0 {
                            self.write_slot1_ep0_output_stopped(mem);
                        }
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_SET_TR_DEQUEUE_POINTER => {
                    let Some(raw_dequeue) = read_u64(&raw_command, 0) else {
                        return false;
                    };
                    let slot_id = command_slot_id(command_control);
                    if slot_id == SLOT_ID {
                        match command_endpoint_id(command_control) {
                            ENDPOINT_ID_EP0 => {
                                self.slot1_ep0_dequeue = raw_dequeue & TR_DEQUEUE_POINTER_MASK;
                                self.slot1_ep0_dcs = raw_dequeue & 1 != 0;
                                self.write_slot1_ep0_output_dequeue(mem);
                            }
                            ENDPOINT_ID_DCI3 => {
                                self.slot1_dci3_dequeue = raw_dequeue & TR_DEQUEUE_POINTER_MASK;
                                self.slot1_dci3_ring_base = self.slot1_dci3_dequeue;
                                self.slot1_dci3_dcs = raw_dequeue & 1 != 0;
                                self.slot1_dci3_two_entry_queue_rearm = false;
                                self.write_slot1_dci3_output_dequeue(mem);
                            }
                            ENDPOINT_ID_DCI5 => {
                                self.slot1_dci5_dequeue = raw_dequeue & TR_DEQUEUE_POINTER_MASK;
                                self.slot1_dci5_ring_base = self.slot1_dci5_dequeue;
                                self.slot1_dci5_dcs = raw_dequeue & 1 != 0;
                                self.write_slot1_dci5_output_dequeue(mem);
                            }
                            _ => {}
                        }
                    }
                    let posted = self.post_command_completion(mem, command_trb, slot_id);
                    if posted {
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_ADDRESS_DEVICE => {
                    let Some(input_context) = read_u64(&raw_command, 0) else {
                        return false;
                    };
                    let slot_id = command_slot_id(command_control);
                    self.capture_address_device_input_context(
                        mem,
                        input_context,
                        slot_id,
                        command_control & ADDRESS_DEVICE_BSR != 0,
                    );
                    let posted = self.post_command_completion(mem, command_trb, slot_id);
                    if posted {
                        if slot_id == SLOT_ID {
                            self.usb_configuration = 0;
                        }
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_CONFIGURE_ENDPOINT => {
                    let Some(input_context) = read_u64(&raw_command, 0) else {
                        return false;
                    };
                    let slot_id = command_slot_id(command_control);
                    self.capture_configure_endpoint_input_context(mem, input_context, slot_id);
                    let posted = self.post_command_completion(mem, command_trb, slot_id);
                    if posted {
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_EVALUATE_CONTEXT => {
                    let slot_id = command_slot_id(command_control);
                    let posted = self.post_command_completion(mem, command_trb, slot_id);
                    if posted {
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_RESET_DEVICE => {
                    // winload resets the BSR-addressed device before the full
                    // Address Device; the slot returns to the default state
                    // with every endpoint but EP0 disabled.
                    let slot_id = command_slot_id(command_control);
                    let posted = self.post_command_completion(mem, command_trb, slot_id);
                    if posted {
                        if slot_id == SLOT_ID {
                            self.usb_configuration = 0;
                            self.invalidate_slot1_dci3_endpoint_state();
                            self.invalidate_slot1_dci5_endpoint_state();
                        }
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                TRB_TYPE_NO_OP_COMMAND => {
                    let posted = self.post_command_completion(mem, command_trb, 0);
                    if posted {
                        self.advance_command_dequeue(command_trb);
                    }
                    return posted;
                }
                _ => return false,
            }
        }
        false
    }

    fn post_command_completion(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        slot_id: u32,
    ) -> bool {
        self.post_event(
            mem,
            command_trb,
            COMPLETION_CODE_SUCCESS << 24,
            event_control(slot_id),
        )
    }

    fn advance_command_dequeue(&mut self, command_trb: u64) {
        if let Some(next) = command_trb.checked_add(TRB_SIZE_BYTES) {
            self.command_dequeue = next;
            self.crcr = next | u64::from(self.command_cycle);
        }
    }

    fn sync_command_ring_from_crcr(&mut self) {
        self.command_dequeue = self.crcr & COMMAND_RING_POINTER_MASK;
        self.command_cycle = self.crcr & 1 != 0;
    }
}

fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
}

fn command_slot_id(control: u32) -> u32 {
    (control >> COMMAND_SLOT_ID_SHIFT) & COMMAND_SLOT_ID_MASK
}

fn command_endpoint_id(control: u32) -> u32 {
    (control >> COMMAND_ENDPOINT_ID_SHIFT) & COMMAND_ENDPOINT_ID_MASK
}

fn event_control(slot_id: u32) -> u32 {
    (slot_id << COMMAND_SLOT_ID_SHIFT) | (TRB_TYPE_COMMAND_COMPLETION_EVENT << TRB_TYPE_SHIFT)
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
