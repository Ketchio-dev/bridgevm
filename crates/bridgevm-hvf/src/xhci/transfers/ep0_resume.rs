//! xHCI 1.2b section 4.8.3: publish Running before any resumed transfer event.

use super::super::device_context_mem::{
    output_context_for_slot, read_mem_u32, write_ep_context_state,
};
use super::*;

const DCBAA_POINTER_MASK: u64 = !0x3f;
const EP0_CONTEXT_OFFSET: u64 = 0x20;
const EP_STATE_MASK: u32 = 0x7;
const EP_STATE_STOPPED: u32 = 3;
const EP_STATE_RUNNING: u32 = 1;

impl XhciController {
    pub(super) fn process_ep0_doorbell(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let context = output_context_for_slot(mem, self.dcbaap & DCBAA_POINTER_MASK, SLOT_ID)
            .and_then(|address| address.checked_add(EP0_CONTEXT_OFFSET));
        if let Some(context) = context {
            if read_mem_u32(mem, context)
                .is_some_and(|state| state & EP_STATE_MASK == EP_STATE_STOPPED)
                && !write_ep_context_state(mem, context, EP_STATE_RUNNING)
            {
                return false;
            }
        }
        self.process_ep0_control_transfer(mem)
    }
}
