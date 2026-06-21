use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

const DOORBELL0: u64 = 0x2000;
const TRB_SIZE: usize = 16;
const TRB_SIZE_BYTES: u64 = 16;
const COMMAND_RING_POINTER_MASK: u64 = !0x3f;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const SLOT_ID: u32 = 1;
const IMAN_INTERRUPT_PENDING: u32 = 1;

pub(super) const fn is_command_doorbell(offset: u64, size: u8) -> bool {
    offset == DOORBELL0 && size == 4
}

impl XhciController {
    pub(super) fn process_command_doorbell(&mut self, mem: &mut dyn GuestMemoryMut) {
        let command_trb = self.crcr & COMMAND_RING_POINTER_MASK;
        let Some(raw_command) = mem.read_bytes(command_trb, TRB_SIZE) else {
            return;
        };
        let Some(command_control) = read_u32(&raw_command, 12) else {
            return;
        };
        if trb_type(command_control) != TRB_TYPE_ENABLE_SLOT {
            return;
        }
        let expected_cycle = if self.crcr & 1 != 0 { TRB_CYCLE } else { 0 };
        if command_control & TRB_CYCLE != expected_cycle {
            return;
        }
        self.post_enable_slot_completion(mem, command_trb);
    }

    fn post_enable_slot_completion(&mut self, mem: &mut dyn GuestMemoryMut, command_trb: u64) {
        if self.erstsz0 == 0 {
            return;
        }
        let Some(raw_erst) = mem.read_bytes(self.erstba0, 16) else {
            return;
        };
        let Some(segment_base) = read_u64(&raw_erst, 0) else {
            return;
        };
        let Some(segment_trbs) = read_u32(&raw_erst, 8) else {
            return;
        };
        if segment_base == 0 || segment_trbs == 0 || self.event_enqueue >= segment_trbs {
            return;
        }

        let event_gpa = segment_base + u64::from(self.event_enqueue) * TRB_SIZE_BYTES;
        let mut event = [0u8; TRB_SIZE];
        event[0..8].copy_from_slice(&command_trb.to_le_bytes());
        event[8..12].copy_from_slice(&(COMPLETION_CODE_SUCCESS << 24).to_le_bytes());
        event[12..16].copy_from_slice(&event_control(self.event_cycle).to_le_bytes());
        if !mem.write_bytes(event_gpa, &event) {
            return;
        }

        self.event_enqueue += 1;
        if self.event_enqueue == segment_trbs {
            self.event_enqueue = 0;
            self.event_cycle = !self.event_cycle;
        }
        self.iman0 |= IMAN_INTERRUPT_PENDING;
    }
}

fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
}

fn event_control(cycle: bool) -> u32 {
    let cycle_bit = if cycle { TRB_CYCLE } else { 0 };
    (SLOT_ID << 24) | (TRB_TYPE_COMMAND_COMPLETION_EVENT << TRB_TYPE_SHIFT) | cycle_bit
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
