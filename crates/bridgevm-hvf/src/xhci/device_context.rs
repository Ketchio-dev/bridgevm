use crate::fwcfg::GuestMemoryMut;

use super::device_context_mem::{
    output_context_for_slot, read_mem_array, read_mem_u64, read_u64, write_ep_context_state,
    write_mem_u32, write_mem_u64,
};
use super::XhciController;

#[path = "device_context_drop_context.rs"]
mod drop_context;

pub(super) const SLOT_ID: u32 = 1;
pub(super) const DCBAA_POINTER_MASK: u64 = !0x3f;
const DEVICE_CONTEXT_POINTER_MASK: u64 = !0x3f;
const SLOT_INPUT_CONTEXT_OFFSET: u64 = 0x20;
const SLOT_CONTEXT_DWORD0_TO_DWORD2_BYTES: usize = 12;
const SLOT_CONTEXT_DWORD3_OFFSET: u64 = 0x0c;
const EP0_INPUT_CONTEXT_OFFSET: u64 = 0x40;
const EP0_OUTPUT_CONTEXT_OFFSET: u64 = 0x20;
pub(super) const DCI3_OUTPUT_CONTEXT_OFFSET: u64 = 0x60;
pub(super) const DCI5_OUTPUT_CONTEXT_OFFSET: u64 = 0xa0;
pub(super) const INPUT_CONTROL_DROP_CONTEXT_OFFSET: u64 = 0x00;
pub(super) const EP_CONTEXT_BYTES: usize = 32;
pub(super) const EP_TR_DEQUEUE_OFFSET: u64 = 0x8;
const EP_TR_DEQUEUE_MASK: u64 = !0xf;
pub(super) const DCI3: u32 = 3;
pub(super) const DCI5: u32 = 5;
pub(super) const EP_STATE_DISABLED: u32 = 0;
const SLOT_STATE_DEFAULT: u32 = 2 << 27;
const SLOT_STATE_ADDRESSED: u32 = 3 << 27;
const EP_STATE_RUNNING: u32 = 1;
const EP_STATE_STOPPED: u32 = 3;

impl XhciController {
    pub(super) fn capture_address_device_input_context(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        input_context: u64,
        slot_id: u32,
        block_set_address_request: bool,
    ) {
        if slot_id != SLOT_ID {
            return;
        }
        let input_context = input_context & DEVICE_CONTEXT_POINTER_MASK;
        let slot_input_context = input_context
            .checked_add(SLOT_INPUT_CONTEXT_OFFSET)
            .and_then(|gpa| read_mem_array::<SLOT_CONTEXT_DWORD0_TO_DWORD2_BYTES>(mem, gpa));
        let Some(ep0_input_gpa) = input_context.checked_add(EP0_INPUT_CONTEXT_OFFSET) else {
            return;
        };
        let Some(ep0_input_context) = read_mem_array::<EP_CONTEXT_BYTES>(mem, ep0_input_gpa) else {
            return;
        };
        let Some(raw_dequeue) = read_u64(&ep0_input_context, EP_TR_DEQUEUE_OFFSET as usize) else {
            return;
        };
        self.slot1_ep0_dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
        self.slot1_ep0_dcs = raw_dequeue & 1 != 0;

        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(output_context) = output_context_for_slot(mem, dcbaa, slot_id) else {
            return;
        };

        let Some(slot_dword3_gpa) = output_context.checked_add(SLOT_CONTEXT_DWORD3_OFFSET) else {
            return;
        };
        let Some(ep0_output_gpa) = output_context.checked_add(EP0_OUTPUT_CONTEXT_OFFSET) else {
            return;
        };
        let Some(ep0_dequeue_output_gpa) = ep0_output_gpa.checked_add(EP_TR_DEQUEUE_OFFSET) else {
            return;
        };
        if read_mem_array::<4>(mem, slot_dword3_gpa).is_none()
            || read_mem_array::<EP_CONTEXT_BYTES>(mem, ep0_output_gpa).is_none()
        {
            return;
        }
        if let Some(slot_input_context) = slot_input_context {
            if !mem.write_bytes(output_context, &slot_input_context) {
                return;
            }
        }
        if block_set_address_request {
            if !write_mem_u32(mem, slot_dword3_gpa, SLOT_STATE_DEFAULT | (slot_id & 0xff)) {
                return;
            }
            if !mem.write_bytes(ep0_output_gpa, &ep0_input_context) {
                return;
            }
            if !write_mem_u64(mem, ep0_dequeue_output_gpa, raw_dequeue) {
                return;
            }
            self.capture_slot1_dci3_input_context(mem, input_context);
            self.capture_slot1_dci5_input_context(mem, input_context);
            return;
        }
        if !write_mem_u32(
            mem,
            slot_dword3_gpa,
            SLOT_STATE_ADDRESSED | (slot_id & 0xff),
        ) {
            return;
        }
        if !mem.write_bytes(ep0_output_gpa, &ep0_input_context) {
            return;
        }
        if !write_ep_context_state(mem, ep0_output_gpa, EP_STATE_RUNNING) {
            return;
        }
        if !write_mem_u64(mem, ep0_dequeue_output_gpa, raw_dequeue) {
            return;
        }
        if !self.capture_slot1_dci3_input_context(mem, input_context)
            && self.slot1_dci3_dequeue != 0
            && self.slot1_dci3_ring_base != 0
        {
            self.write_slot1_dci3_output_dequeue(mem);
        }
        self.capture_slot1_dci5_input_context(mem, input_context);
    }

    pub(super) fn capture_configure_endpoint_input_context(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        input_context: u64,
        slot_id: u32,
    ) {
        if slot_id != SLOT_ID {
            return;
        }
        let input_context = input_context & DEVICE_CONTEXT_POINTER_MASK;
        self.apply_slot1_configure_endpoint_drop_context(mem, input_context);
        self.capture_slot1_dci3_input_context(mem, input_context);
        self.capture_slot1_dci5_input_context(mem, input_context);
    }

    pub(super) fn write_slot1_dci3_output_dequeue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.remember_slot1_dci3_endpoint_state();
        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(output_context) = output_context_for_slot(mem, dcbaa, SLOT_ID) else {
            return;
        };
        let Some(dci3_dequeue_gpa) = output_context
            .checked_add(DCI3_OUTPUT_CONTEXT_OFFSET)
            .and_then(|gpa| gpa.checked_add(EP_TR_DEQUEUE_OFFSET))
        else {
            return;
        };
        write_mem_u64(
            mem,
            dci3_dequeue_gpa,
            self.slot1_dci3_dequeue | u64::from(self.slot1_dci3_dcs),
        );
    }

    pub(super) fn write_slot1_ep0_output_dequeue(&self, mem: &mut dyn GuestMemoryMut) {
        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(output_context) = output_context_for_slot(mem, dcbaa, SLOT_ID) else {
            return;
        };
        let Some(ep0_output_gpa) = output_context.checked_add(EP0_OUTPUT_CONTEXT_OFFSET) else {
            return;
        };
        let Some(ep0_dequeue_gpa) = ep0_output_gpa.checked_add(EP_TR_DEQUEUE_OFFSET) else {
            return;
        };
        if read_mem_array::<EP_CONTEXT_BYTES>(mem, ep0_output_gpa).is_none() {
            return;
        }
        write_mem_u64(
            mem,
            ep0_dequeue_gpa,
            self.slot1_ep0_dequeue | u64::from(self.slot1_ep0_dcs),
        );
    }

    pub(super) fn write_slot1_ep0_output_stopped(&self, mem: &mut dyn GuestMemoryMut) {
        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(output_context) = output_context_for_slot(mem, dcbaa, SLOT_ID) else {
            return;
        };
        let Some(ep0_output_gpa) = output_context.checked_add(EP0_OUTPUT_CONTEXT_OFFSET) else {
            return;
        };
        if read_mem_array::<EP_CONTEXT_BYTES>(mem, ep0_output_gpa).is_none() {
            return;
        }
        if !write_ep_context_state(mem, ep0_output_gpa, EP_STATE_STOPPED) {
            return;
        }
        self.write_slot1_ep0_output_dequeue(mem);
    }

    pub(super) fn disable_slot1_context(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.slot1_ep0_dequeue = 0;
        self.slot1_ep0_dcs = false;
        self.invalidate_slot1_dci3_endpoint_state();
        self.invalidate_slot1_dci5_endpoint_state();
        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(output_context) = output_context_for_slot(mem, dcbaa, SLOT_ID) else {
            return;
        };
        for endpoint_offset in [
            EP0_OUTPUT_CONTEXT_OFFSET,
            DCI3_OUTPUT_CONTEXT_OFFSET,
            DCI5_OUTPUT_CONTEXT_OFFSET,
        ] {
            let Some(endpoint_gpa) = output_context.checked_add(endpoint_offset) else {
                continue;
            };
            let Some(dequeue_gpa) = endpoint_gpa.checked_add(EP_TR_DEQUEUE_OFFSET) else {
                continue;
            };
            if read_mem_array::<EP_CONTEXT_BYTES>(mem, endpoint_gpa).is_none() {
                continue;
            }
            if !write_ep_context_state(mem, endpoint_gpa, EP_STATE_DISABLED) {
                return;
            }
            if !write_mem_u64(mem, dequeue_gpa, 0) {
                return;
            }
        }
    }

    pub(super) fn slot1_dci3_output_dequeue_state(
        &self,
        mem: &dyn GuestMemoryMut,
    ) -> Option<(u64, bool)> {
        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let output_context = output_context_for_slot(mem, dcbaa, SLOT_ID)?;
        let dci3_dequeue_gpa = output_context
            .checked_add(DCI3_OUTPUT_CONTEXT_OFFSET)?
            .checked_add(EP_TR_DEQUEUE_OFFSET)?;
        let raw_dequeue = read_mem_u64(mem, dci3_dequeue_gpa)?;
        let dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
        (dequeue != 0).then_some((dequeue, raw_dequeue & 1 != 0))
    }
}
