use bridgevm_hvf::fwcfg::GuestMemoryMut;

use super::trb;

#[path = "context_interrupt.rs"]
mod context_interrupt;

const INPUT_CONTEXT_POINTER_MASK: u64 = !0xf;
pub(super) const TRANSFER_RING_POINTER_MASK: u64 = !0xf;
const INPUT_CONTROL_DROP_CONTEXT_OFFSET: u64 = 0x00;
pub(super) const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const INPUT_CONTROL_CONTEXT_BYTES: u64 = 0x20;
const SLOT_CONTEXT_BYTES: u64 = 0x20;
pub(super) const EP0_CONTEXT_OFFSET: u64 = INPUT_CONTROL_CONTEXT_BYTES + SLOT_CONTEXT_BYTES;
const EP0: u32 = 1;
pub(super) const DCI3: u32 = 3;
pub(super) const DCI3_INPUT_CONTEXT_OFFSET: u64 =
    INPUT_CONTROL_CONTEXT_BYTES + SLOT_CONTEXT_BYTES * 3;
pub(super) const DCI5: u32 = 5;
pub(super) const DCI5_INPUT_CONTEXT_OFFSET: u64 =
    INPUT_CONTROL_CONTEXT_BYTES + SLOT_CONTEXT_BYTES * 5;
pub(super) const EP_TR_DEQUEUE_OFFSET: u64 = 0x8;
const EP_CONTEXT_DWORD1_OFFSET: u64 = 0x4;
const EP_CONTEXT_DWORD4_OFFSET: u64 = 0x10;
const SLOT_COUNT: usize = 256;
const DCI3_CONTEXT_MASK: u32 = 1 << DCI3;
const DCI5_CONTEXT_MASK: u32 = 1 << DCI5;

#[derive(Debug)]
pub(super) struct EndpointContexts {
    ep0_dequeue_by_slot: [u64; SLOT_COUNT],
    dci3_dequeue_by_slot: [u64; SLOT_COUNT],
    dci3_dcs_by_slot: [bool; SLOT_COUNT],
    dci5_dequeue_by_slot: [u64; SLOT_COUNT],
    dci5_dcs_by_slot: [bool; SLOT_COUNT],
}

impl EndpointContexts {
    pub(super) const fn new() -> Self {
        Self {
            ep0_dequeue_by_slot: [0; SLOT_COUNT],
            dci3_dequeue_by_slot: [0; SLOT_COUNT],
            dci3_dcs_by_slot: [false; SLOT_COUNT],
            dci5_dequeue_by_slot: [0; SLOT_COUNT],
            dci5_dcs_by_slot: [false; SLOT_COUNT],
        }
    }

    pub(super) fn ep0_dequeue(&self, slot: usize) -> u64 {
        self.ep0_dequeue_by_slot[slot]
    }

    pub(super) fn dci3_dequeue(&self, slot: usize) -> u64 {
        self.dci3_dequeue_by_slot[slot]
    }

    pub(super) fn dci5_dequeue(&self, slot: usize) -> u64 {
        self.dci5_dequeue_by_slot[slot]
    }

    pub(super) fn ring_for_target(&self, slot: usize, target: u32) -> (&'static str, u64) {
        match target {
            EP0 => ("ep0", self.ep0_dequeue(slot)),
            DCI3 => ("dci3", self.dci3_dequeue(slot)),
            DCI5 => ("dci5", self.dci5_dequeue(slot)),
            _ => ("unknown", 0),
        }
    }

    pub(super) fn set_tr_dequeue_pointer(
        &mut self,
        slot: u32,
        endpoint: u32,
        raw_dequeue: u64,
    ) -> Option<String> {
        let slot_index = usize::try_from(slot).ok()?;
        if slot_index >= SLOT_COUNT {
            return None;
        }
        let dequeue = raw_dequeue & TRANSFER_RING_POINTER_MASK;
        let dcs = raw_dequeue & u64::from(trb::CYCLE) != 0;
        match endpoint {
            EP0 => {
                self.ep0_dequeue_by_slot[slot_index] = dequeue;
                Some(format!(
                    "set_tr_dequeue_pointer slot={slot} endpoint={endpoint} raw_dequeue={raw_dequeue:#x} ep0_dequeue={dequeue:#x}"
                ))
            }
            DCI3 => {
                self.dci3_dequeue_by_slot[slot_index] = dequeue;
                self.dci3_dcs_by_slot[slot_index] = dcs;
                Some(format!(
                    "set_tr_dequeue_pointer slot={slot} endpoint={endpoint} raw_dequeue={raw_dequeue:#x} dci3_dequeue={dequeue:#x} dci3_dcs={dcs}"
                ))
            }
            DCI5 => {
                self.dci5_dequeue_by_slot[slot_index] = dequeue;
                self.dci5_dcs_by_slot[slot_index] = dcs;
                Some(format!(
                    "set_tr_dequeue_pointer slot={slot} endpoint={endpoint} raw_dequeue={raw_dequeue:#x} dci5_dequeue={dequeue:#x} dci5_dcs={dcs}"
                ))
            }
            _ => Some(format!(
                "set_tr_dequeue_pointer slot={slot} endpoint={endpoint} raw_dequeue={raw_dequeue:#x} ignored=true"
            )),
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
            self.set_interrupt_in_context(slot_index, DCI3, 0, false);
        }
        if drop_context & DCI5_CONTEXT_MASK != 0 {
            self.set_interrupt_in_context(slot_index, DCI5, 0, false);
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
        let dci3 = self.capture_interrupt_in_context(
            slot_index,
            input_context,
            drop_context,
            add_context,
            DCI3,
            "dci3",
            DCI3_INPUT_CONTEXT_OFFSET,
            mem,
        );
        let dci5 = self.capture_interrupt_in_context(
            slot_index,
            input_context,
            drop_context,
            add_context,
            DCI5,
            "dci5",
            DCI5_INPUT_CONTEXT_OFFSET,
            mem,
        );
        Some(format!(
            "configure_endpoint slot={slot} input_context={input_context:#x} drop_context={drop_context:#x} add_context={add_context:#x} {dci3} {dci5}"
        ))
    }
}
