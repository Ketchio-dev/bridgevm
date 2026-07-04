use bridgevm_hvf::fwcfg::GuestMemoryMut;

use super::super::trb;
use super::{
    EndpointContexts, DCI3, DCI5, EP_CONTEXT_DWORD1_OFFSET, EP_CONTEXT_DWORD4_OFFSET,
    EP_TR_DEQUEUE_OFFSET, TRANSFER_RING_POINTER_MASK,
};

impl EndpointContexts {
    pub(super) fn capture_interrupt_in_context(
        &mut self,
        slot_index: usize,
        input_context: u64,
        drop_context: u32,
        add_context: u32,
        endpoint: u32,
        name: &'static str,
        context_offset: u64,
        mem: &dyn GuestMemoryMut,
    ) -> String {
        let context_mask = 1_u32 << endpoint;
        if drop_context & context_mask != 0 {
            self.set_interrupt_in_context(slot_index, endpoint, 0, false);
            return format!("{name}=dropped");
        }
        if add_context & context_mask == 0 {
            return format!("{name}=not-added");
        }
        let Some(context) = input_context.checked_add(context_offset) else {
            return format!("{name}_context=overflow {name}=unreadable");
        };
        let Some(dword1_gpa) = context.checked_add(EP_CONTEXT_DWORD1_OFFSET) else {
            return format!("{name}_context={context:#x} {name}=overflow");
        };
        let Some(dword1) = trb::read_guest_u32(mem, dword1_gpa) else {
            return format!("{name}_context={context:#x} {name}=unreadable");
        };
        let Some(dequeue_gpa) = context.checked_add(EP_TR_DEQUEUE_OFFSET) else {
            return format!("{name}_context={context:#x} {name}_dequeue=overflow");
        };
        let Some(dequeue_raw) = trb::read_guest_u64(mem, dequeue_gpa) else {
            return format!("{name}_context={context:#x} {name}_dequeue=unreadable");
        };
        let dword4 = context
            .checked_add(EP_CONTEXT_DWORD4_OFFSET)
            .and_then(|gpa| trb::read_guest_u32(mem, gpa));
        let dequeue = dequeue_raw & TRANSFER_RING_POINTER_MASK;
        let dcs = dequeue_raw & u64::from(trb::CYCLE) != 0;
        self.set_interrupt_in_context(slot_index, endpoint, dequeue, dcs);
        match dword4 {
            Some(dword4) => format!(
                "{name}_context={context:#x} {name}_dword1={dword1:#x} {name}_dequeue_raw={dequeue_raw:#x} {name}_dequeue={dequeue:#x} {name}_dcs={dcs} {name}_dword4={dword4:#x}"
            ),
            None => format!(
                "{name}_context={context:#x} {name}_dword1={dword1:#x} {name}_dequeue_raw={dequeue_raw:#x} {name}_dequeue={dequeue:#x} {name}_dcs={dcs} {name}_dword4=unreadable"
            ),
        }
    }

    pub(super) fn set_interrupt_in_context(
        &mut self,
        slot_index: usize,
        endpoint: u32,
        dequeue: u64,
        dcs: bool,
    ) {
        match endpoint {
            DCI3 => {
                self.dci3_dequeue_by_slot[slot_index] = dequeue;
                self.dci3_dcs_by_slot[slot_index] = dcs;
            }
            DCI5 => {
                self.dci5_dequeue_by_slot[slot_index] = dequeue;
                self.dci5_dcs_by_slot[slot_index] = dcs;
            }
            _ => {}
        }
    }
}
