use crate::fwcfg::GuestMemoryMut;

use super::device_context_mem::{output_context_for_slot, read_mem_u32, read_u64};
use super::XhciController;

const SLOT_ID: u32 = 1;
const DCBAA_POINTER_MASK: u64 = !0x3f;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const DCI3: u32 = 3;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = 0x80;
const DCI3_OUTPUT_CONTEXT_OFFSET: u64 = 0x60;
const EP_CONTEXT_BYTES: usize = 32;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x08;
const EP_TR_DEQUEUE_MASK: u64 = !0xf;

impl XhciController {
    pub(super) fn invalidate_slot1_dci3_endpoint_state(&mut self) {
        self.slot1_dci3_dequeue = 0;
        self.slot1_dci3_ring_base = 0;
        self.slot1_dci3_dcs = false;
        self.slot1_dci3_two_entry_queue_rearm = false;
        self.slot1_dci3_last_dequeue = 0;
        self.slot1_dci3_last_dcs = false;
        self.slot1_dci3_last_ring_base = 0;
        self.slot1_dci3_last_ring_dcs = false;
        self.slot1_dci3_last_reusable = false;
    }

    pub(super) fn remember_slot1_dci3_endpoint_state(&mut self) {
        if self.slot1_dci3_dequeue != 0 && self.slot1_dci3_ring_base != 0 {
            self.slot1_dci3_last_dequeue = self.slot1_dci3_dequeue;
            self.slot1_dci3_last_dcs = self.slot1_dci3_dcs;
            self.slot1_dci3_last_ring_base = self.slot1_dci3_ring_base;
            self.slot1_dci3_last_ring_dcs = self.slot1_dci3_dcs;
            self.slot1_dci3_last_reusable = true;
        }
    }

    pub(super) fn capture_slot1_dci3_input_context(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        input_context: u64,
    ) -> bool {
        if input_context
            .checked_add(INPUT_CONTROL_ADD_CONTEXT_OFFSET)
            .and_then(|gpa| read_mem_u32(mem, gpa))
            .is_none_or(|add_context| add_context & (1 << DCI3) == 0)
        {
            return false;
        }
        let Some(dci3_input_context) = input_context
            .checked_add(DCI3_INPUT_CONTEXT_OFFSET)
            .and_then(|gpa| mem.read_bytes(gpa, EP_CONTEXT_BYTES))
        else {
            return false;
        };
        let Some(raw_dequeue) = read_u64(&dci3_input_context, EP_TR_DEQUEUE_OFFSET as usize) else {
            return false;
        };
        let Some(dci3_output_gpa) =
            output_context_for_slot(mem, self.dcbaap & DCBAA_POINTER_MASK, SLOT_ID)
                .and_then(|output_context| output_context.checked_add(DCI3_OUTPUT_CONTEXT_OFFSET))
        else {
            return false;
        };
        if !mem.write_bytes(dci3_output_gpa, &dci3_input_context) {
            return false;
        }
        let dci3_dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
        self.slot1_dci3_dequeue = dci3_dequeue;
        self.slot1_dci3_ring_base = dci3_dequeue;
        self.slot1_dci3_dcs = raw_dequeue & 1 != 0;
        self.slot1_dci3_two_entry_queue_rearm = false;
        self.remember_slot1_dci3_endpoint_state();
        true
    }
}
