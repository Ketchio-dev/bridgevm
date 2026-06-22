use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

mod completion;
mod control;
mod trb;

const DOORBELL_BASE: u64 = 0x2000;
const DOORBELL_STRIDE: u64 = 4;
const SLOT_ID: u32 = 1;
const ENDPOINT_ID_EP0: u32 = 1;
const ENDPOINT_ID_DCI3: u32 = 3;

pub(super) const fn is_slot_doorbell(offset: u64, size: u8) -> bool {
    offset == DOORBELL_BASE + DOORBELL_STRIDE && size == 4
}

impl XhciController {
    pub(super) fn process_slot_doorbell(
        &mut self,
        offset: u64,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        let Some(slot_offset) = offset.checked_sub(DOORBELL_BASE) else {
            return false;
        };
        let Ok(slot_id) = u32::try_from(slot_offset / DOORBELL_STRIDE) else {
            return false;
        };
        let Ok(target) = u32::try_from(value & 0xff) else {
            return false;
        };
        if slot_id != SLOT_ID {
            return false;
        }
        match target {
            ENDPOINT_ID_EP0 => self.process_ep0_control_transfer(mem),
            ENDPOINT_ID_DCI3 => self.process_dci3_interrupt_in_transfer(mem),
            _ => false,
        }
    }
}
