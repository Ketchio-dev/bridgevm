use crate::fwcfg::GuestMemoryMut;

use super::super::device_context_mem::{
    output_context_for_slot, read_mem_array, read_mem_u32, write_ep_context_state, write_mem_u64,
};
use super::super::XhciController;
use super::{
    DCBAA_POINTER_MASK, DCI3, DCI3_OUTPUT_CONTEXT_OFFSET, DCI5, DCI5_OUTPUT_CONTEXT_OFFSET,
    EP_CONTEXT_BYTES, EP_STATE_DISABLED, EP_TR_DEQUEUE_OFFSET, INPUT_CONTROL_DROP_CONTEXT_OFFSET,
    SLOT_ID,
};

impl XhciController {
    pub(super) fn apply_slot1_configure_endpoint_drop_context(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        input_context: u64,
    ) {
        let Some(drop_context) = input_context
            .checked_add(INPUT_CONTROL_DROP_CONTEXT_OFFSET)
            .and_then(|gpa| read_mem_u32(mem, gpa))
        else {
            return;
        };
        if drop_context & (1 << DCI3) != 0 {
            self.invalidate_slot1_dci3_endpoint_state();
            self.disable_slot1_endpoint_output_context(mem, DCI3_OUTPUT_CONTEXT_OFFSET);
        }
        if drop_context & (1 << DCI5) != 0 {
            self.invalidate_slot1_dci5_endpoint_state();
            self.disable_slot1_endpoint_output_context(mem, DCI5_OUTPUT_CONTEXT_OFFSET);
        }
    }

    fn disable_slot1_endpoint_output_context(
        &self,
        mem: &mut dyn GuestMemoryMut,
        endpoint_output_context_offset: u64,
    ) {
        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(output_context) = output_context_for_slot(mem, dcbaa, SLOT_ID) else {
            return;
        };
        let Some(endpoint_gpa) = output_context.checked_add(endpoint_output_context_offset) else {
            return;
        };
        let Some(dequeue_gpa) = endpoint_gpa.checked_add(EP_TR_DEQUEUE_OFFSET) else {
            return;
        };
        if read_mem_array::<EP_CONTEXT_BYTES>(mem, endpoint_gpa).is_none() {
            return;
        }
        if !write_ep_context_state(mem, endpoint_gpa, EP_STATE_DISABLED) {
            return;
        }
        let _ = write_mem_u64(mem, dequeue_gpa, 0);
    }
}
