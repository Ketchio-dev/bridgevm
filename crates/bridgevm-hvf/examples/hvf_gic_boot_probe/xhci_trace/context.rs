use bridgevm_hvf::fwcfg::GuestMemoryMut;

use super::trb;

const INPUT_CONTEXT_POINTER_MASK: u64 = !0xf;
const TRANSFER_RING_POINTER_MASK: u64 = !0xf;
const INPUT_CONTROL_DROP_CONTEXT_OFFSET: u64 = 0x00;
pub(super) const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const INPUT_CONTROL_CONTEXT_BYTES: u64 = 0x20;
const SLOT_CONTEXT_BYTES: u64 = 0x20;
pub(super) const EP0_CONTEXT_OFFSET: u64 = INPUT_CONTROL_CONTEXT_BYTES + SLOT_CONTEXT_BYTES;
pub(super) const DCI3: u32 = 3;
pub(super) const DCI3_INPUT_CONTEXT_OFFSET: u64 =
    INPUT_CONTROL_CONTEXT_BYTES + SLOT_CONTEXT_BYTES * 3;
pub(super) const EP_TR_DEQUEUE_OFFSET: u64 = 0x8;
const EP_CONTEXT_DWORD1_OFFSET: u64 = 0x4;
const EP_CONTEXT_DWORD4_OFFSET: u64 = 0x10;
const SLOT_COUNT: usize = 256;
const DCI3_CONTEXT_MASK: u32 = 1 << DCI3;

#[derive(Debug)]
pub(super) struct EndpointContexts {
    ep0_dequeue_by_slot: [u64; SLOT_COUNT],
    dci3_dequeue_by_slot: [u64; SLOT_COUNT],
    dci3_dcs_by_slot: [bool; SLOT_COUNT],
}

impl EndpointContexts {
    pub(super) const fn new() -> Self {
        Self {
            ep0_dequeue_by_slot: [0; SLOT_COUNT],
            dci3_dequeue_by_slot: [0; SLOT_COUNT],
            dci3_dcs_by_slot: [false; SLOT_COUNT],
        }
    }

    pub(super) fn ep0_dequeue(&self, slot: usize) -> u64 {
        self.ep0_dequeue_by_slot[slot]
    }

    pub(super) fn dci3_dequeue(&self, slot: usize) -> u64 {
        self.dci3_dequeue_by_slot[slot]
    }

    pub(super) fn ring_for_target(&self, slot: usize, target: u32) -> (&'static str, u64) {
        match target {
            1 => ("ep0", self.ep0_dequeue(slot)),
            DCI3 => ("dci3", self.dci3_dequeue(slot)),
            _ => ("unknown", 0),
        }
    }

    pub(super) fn capture_address_device(
        &mut self,
        slot: u32,
        input_context: u64,
        mem: &dyn GuestMemoryMut,
    ) -> Option<String> {
        let slot_index = usize::try_from(slot).ok()?;
        if slot_index >= SLOT_COUNT {
            return None;
        }
        let input_context = input_context & INPUT_CONTEXT_POINTER_MASK;
        let Some(ep0_context) = input_context.checked_add(EP0_CONTEXT_OFFSET) else {
            return Some(format!(
                "address_device slot={slot} input_context={input_context:#x} ep0_context=overflow ep0_dequeue=unreadable"
            ));
        };
        let Some(ep0_dequeue_gpa) = ep0_context.checked_add(EP_TR_DEQUEUE_OFFSET) else {
            return Some(format!(
                "address_device slot={slot} input_context={input_context:#x} ep0_context={ep0_context:#x} ep0_dequeue=overflow"
            ));
        };
        let Some(ep0_dequeue_raw) = trb::read_guest_u64(mem, ep0_dequeue_gpa) else {
            return Some(format!(
                "address_device slot={slot} input_context={input_context:#x} ep0_context={ep0_context:#x} ep0_dequeue=unreadable"
            ));
        };
        let ep0_dequeue = ep0_dequeue_raw & TRANSFER_RING_POINTER_MASK;
        self.ep0_dequeue_by_slot[slot_index] = ep0_dequeue;
        Some(format!(
            "address_device slot={slot} input_context={input_context:#x} ep0_context={ep0_context:#x} ep0_dequeue_raw={ep0_dequeue_raw:#x} ep0_dequeue={ep0_dequeue:#x}"
        ))
    }

    pub(super) fn capture_configure_endpoint(
        &mut self,
        slot: u32,
        input_context: u64,
        mem: &dyn GuestMemoryMut,
    ) -> Option<String> {
        let slot_index = usize::try_from(slot).ok()?;
        if slot_index >= SLOT_COUNT {
            return None;
        }
        let input_context = input_context & INPUT_CONTEXT_POINTER_MASK;
        let Some(drop_context_gpa) = input_context.checked_add(INPUT_CONTROL_DROP_CONTEXT_OFFSET)
        else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context=overflow"
            ));
        };
        let Some(drop_context) = trb::read_guest_u32(mem, drop_context_gpa) else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context=unreadable"
            ));
        };
        if drop_context & DCI3_CONTEXT_MASK != 0 {
            self.dci3_dequeue_by_slot[slot_index] = 0;
            self.dci3_dcs_by_slot[slot_index] = false;
        }
        let Some(add_context_gpa) = input_context.checked_add(INPUT_CONTROL_ADD_CONTEXT_OFFSET)
        else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context=overflow"
            ));
        };
        let Some(add_context) = trb::read_guest_u32(mem, add_context_gpa) else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context=unreadable"
            ));
        };
        if drop_context & DCI3_CONTEXT_MASK != 0 {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3=dropped"
            ));
        }
        if add_context & DCI3_CONTEXT_MASK == 0 {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3=not-added"
            ));
        }
        let Some(dci3_context) = input_context.checked_add(DCI3_INPUT_CONTEXT_OFFSET) else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3_context=overflow dci3=unreadable"
            ));
        };
        let Some(dci3_dword1_gpa) = dci3_context.checked_add(EP_CONTEXT_DWORD1_OFFSET) else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3_context={dci3_context:#x} dci3=overflow"
            ));
        };
        let Some(dci3_dword1) = trb::read_guest_u32(mem, dci3_dword1_gpa) else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3_context={dci3_context:#x} dci3=unreadable"
            ));
        };
        let Some(dci3_dequeue_gpa) = dci3_context.checked_add(EP_TR_DEQUEUE_OFFSET) else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3_context={dci3_context:#x} dci3_dequeue=overflow"
            ));
        };
        let Some(dci3_dequeue_raw) = trb::read_guest_u64(mem, dci3_dequeue_gpa) else {
            return Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3_context={dci3_context:#x} dci3_dequeue=unreadable"
            ));
        };
        let dci3_dword4 = dci3_context
            .checked_add(EP_CONTEXT_DWORD4_OFFSET)
            .and_then(|gpa| trb::read_guest_u32(mem, gpa));
        let dci3_dequeue = dci3_dequeue_raw & TRANSFER_RING_POINTER_MASK;
        let dci3_dcs = dci3_dequeue_raw & u64::from(trb::CYCLE) != 0;
        self.dci3_dequeue_by_slot[slot_index] = dci3_dequeue;
        self.dci3_dcs_by_slot[slot_index] = dci3_dcs;
        match dci3_dword4 {
            Some(dci3_dword4) => Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3_context={dci3_context:#x} dci3_dword1={dci3_dword1:#x} dci3_dequeue_raw={dci3_dequeue_raw:#x} dci3_dequeue={dci3_dequeue:#x} dci3_dcs={dci3_dcs} dci3_dword4={dci3_dword4:#x}"
            )),
            None => Some(format!(
                "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} dci3_context={dci3_context:#x} dci3_dword1={dci3_dword1:#x} dci3_dequeue_raw={dci3_dequeue_raw:#x} dci3_dequeue={dci3_dequeue:#x} dci3_dcs={dci3_dcs} dci3_dword4=unreadable"
            )),
        }
    }
}
