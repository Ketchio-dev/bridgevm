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
const TRB_TYPE_RESET_ENDPOINT: u32 = 14;
const TRB_TYPE_STOP_ENDPOINT: u32 = 15;
const TRB_TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const TRB_TYPE_RESET_DEVICE: u32 = 17;
const TRB_TYPE_NO_OP_COMMAND: u32 = 23;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const COMPLETION_CODE_SLOT_NOT_ENABLED: u32 = 11;
const COMPLETION_CODE_ENDPOINT_NOT_ENABLED: u32 = 12;
const COMPLETION_CODE_CONTEXT_STATE: u32 = 19;
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
            let [p0, p1, p2, p3, p4, p5, p6, p7, _, _, _, _, c0, c1, c2, c3] = raw_command;
            let parameter = u64::from_le_bytes([p0, p1, p2, p3, p4, p5, p6, p7]);
            let control = u32::from_le_bytes([c0, c1, c2, c3]);
            let expected_cycle = if self.command_cycle { TRB_CYCLE } else { 0 };
            if control & TRB_CYCLE != expected_cycle {
                return false;
            }
            if trb_type(control) != TRB_TYPE_LINK {
                return self.execute_command(mem, command_trb, parameter, control);
            }
            self.command_dequeue = parameter & LINK_TRB_POINTER_MASK;
            if control & TRB_LINK_TOGGLE_CYCLE != 0 {
                self.command_cycle = !self.command_cycle;
            }
        }
        false
    }

    /// Runs the command TRB at `command_trb`. Returns whether its Command
    /// Completion Event was posted; only then does the ring move past it.
    fn execute_command(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        parameter: u64,
        control: u32,
    ) -> bool {
        let slot_id = command_slot_id(control);
        let endpoint_id = command_endpoint_id(control);
        match trb_type(control) {
            TRB_TYPE_ENABLE_SLOT => {
                self.complete_command(mem, command_trb, SLOT_ID, COMPLETION_CODE_SUCCESS)
            }
            TRB_TYPE_DISABLE_SLOT => self.disable_slot(mem, command_trb, slot_id),
            TRB_TYPE_ADDRESS_DEVICE => {
                let block_set_address_request = control & ADDRESS_DEVICE_BSR != 0;
                self.address_device(
                    mem,
                    command_trb,
                    slot_id,
                    parameter,
                    block_set_address_request,
                )
            }
            TRB_TYPE_CONFIGURE_ENDPOINT => {
                self.capture_configure_endpoint_input_context(mem, parameter, slot_id);
                self.complete_command(mem, command_trb, slot_id, COMPLETION_CODE_SUCCESS)
            }
            TRB_TYPE_EVALUATE_CONTEXT => {
                self.complete_command(mem, command_trb, slot_id, COMPLETION_CODE_SUCCESS)
            }
            TRB_TYPE_RESET_ENDPOINT => {
                let code = self.reset_endpoint(mem, slot_id, endpoint_id);
                self.complete_command(mem, command_trb, slot_id, code)
            }
            TRB_TYPE_STOP_ENDPOINT => self.stop_endpoint(mem, command_trb, slot_id, endpoint_id),
            TRB_TYPE_SET_TR_DEQUEUE_POINTER => {
                self.set_tr_dequeue_pointer(mem, command_trb, slot_id, endpoint_id, parameter)
            }
            TRB_TYPE_RESET_DEVICE => self.reset_device(mem, command_trb, slot_id),
            TRB_TYPE_NO_OP_COMMAND => {
                self.complete_command(mem, command_trb, 0, COMPLETION_CODE_SUCCESS)
            }
            _ => false,
        }
    }

    fn disable_slot(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        slot_id: u32,
    ) -> bool {
        let posted = self.complete_command(mem, command_trb, slot_id, COMPLETION_CODE_SUCCESS);
        if posted && slot_id == SLOT_ID {
            self.usb_configuration = 0;
            self.disable_slot1_context(mem);
        }
        posted
    }

    fn address_device(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        slot_id: u32,
        input_context: u64,
        block_set_address_request: bool,
    ) -> bool {
        self.capture_address_device_input_context(
            mem,
            input_context,
            slot_id,
            block_set_address_request,
        );
        let posted = self.complete_command(mem, command_trb, slot_id, COMPLETION_CODE_SUCCESS);
        if posted && slot_id == SLOT_ID {
            self.usb_configuration = 0;
        }
        posted
    }

    /// Reset Endpoint moves a halted HID endpoint to Stopped; returns the
    /// completion code.
    fn reset_endpoint(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        slot_id: u32,
        endpoint_id: u32,
    ) -> u32 {
        if slot_id != SLOT_ID {
            return COMPLETION_CODE_SLOT_NOT_ENABLED;
        }
        let Some(i) = Self::hid_index(endpoint_id) else {
            return COMPLETION_CODE_ENDPOINT_NOT_ENABLED;
        };
        match self.hid_endpoints[i].state {
            0 => COMPLETION_CODE_ENDPOINT_NOT_ENABLED,
            2 => {
                self.hid_endpoints[i].pending = None;
                self.hid_endpoints[i].state = 3;
                self.publish_hid_state(mem, endpoint_id);
                COMPLETION_CODE_SUCCESS
            }
            _ => COMPLETION_CODE_CONTEXT_STATE,
        }
    }

    fn stop_endpoint(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        slot_id: u32,
        endpoint_id: u32,
    ) -> bool {
        if slot_id == SLOT_ID {
            if let Some(i) = Self::hid_index(endpoint_id) {
                if self.hid_endpoints[i].state == 0 {
                    let code = COMPLETION_CODE_ENDPOINT_NOT_ENABLED;
                    return self.complete_command(mem, command_trb, slot_id, code);
                }
                if !self.stop_hid(mem, endpoint_id) {
                    return false;
                }
            }
        }
        let posted = self.complete_command(mem, command_trb, slot_id, COMPLETION_CODE_SUCCESS);
        if posted && slot_id == SLOT_ID && endpoint_id == ENDPOINT_ID_EP0 {
            self.write_slot1_ep0_output_stopped(mem);
        }
        posted
    }

    fn set_tr_dequeue_pointer(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        slot_id: u32,
        endpoint_id: u32,
        raw_dequeue: u64,
    ) -> bool {
        if slot_id == SLOT_ID {
            if let Some(i) = Self::hid_index(endpoint_id) {
                let state = self.hid_endpoints[i].state;
                if state != 3 {
                    let code = if state == 0 {
                        COMPLETION_CODE_ENDPOINT_NOT_ENABLED
                    } else {
                        COMPLETION_CODE_CONTEXT_STATE
                    };
                    return self.complete_command(mem, command_trb, slot_id, code);
                }
                self.hid_endpoints[i].pending = None;
            }
            self.set_slot1_tr_dequeue(mem, endpoint_id, raw_dequeue);
        }
        self.complete_command(mem, command_trb, slot_id, COMPLETION_CODE_SUCCESS)
    }

    fn set_slot1_tr_dequeue(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        endpoint_id: u32,
        raw_dequeue: u64,
    ) {
        let dequeue = raw_dequeue & TR_DEQUEUE_POINTER_MASK;
        let dcs = raw_dequeue & 1 != 0;
        match endpoint_id {
            ENDPOINT_ID_EP0 => {
                self.slot1_ep0_dequeue = dequeue;
                self.slot1_ep0_dcs = dcs;
                self.write_slot1_ep0_output_dequeue(mem);
            }
            ENDPOINT_ID_DCI3 => {
                self.slot1_dci3_dequeue = dequeue;
                self.slot1_dci3_ring_base = dequeue;
                self.slot1_dci3_dcs = dcs;
                self.slot1_dci3_two_entry_queue_rearm = false;
                self.write_slot1_dci3_output_dequeue(mem);
            }
            ENDPOINT_ID_DCI5 => {
                self.slot1_dci5_dequeue = dequeue;
                self.slot1_dci5_ring_base = dequeue;
                self.slot1_dci5_dcs = dcs;
                self.write_slot1_dci5_output_dequeue(mem);
            }
            _ => {}
        }
    }

    /// winload resets the BSR-addressed device before the full Address
    /// Device; the slot returns to the default state with every endpoint but
    /// EP0 disabled.
    fn reset_device(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        slot_id: u32,
    ) -> bool {
        let posted = self.complete_command(mem, command_trb, slot_id, COMPLETION_CODE_SUCCESS);
        if posted && slot_id == SLOT_ID {
            self.usb_configuration = 0;
            self.invalidate_slot1_dci3_endpoint_state();
            self.invalidate_slot1_dci5_endpoint_state();
        }
        posted
    }

    /// Posts the command's Command Completion Event; once it is posted the
    /// command ring dequeue pointer moves past the command.
    fn complete_command(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        command_trb: u64,
        slot_id: u32,
        code: u32,
    ) -> bool {
        let posted = self.post_event(mem, command_trb, code << 24, event_control(slot_id));
        if posted {
            self.advance_command_dequeue(command_trb);
        }
        posted
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
