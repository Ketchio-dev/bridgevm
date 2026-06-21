use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

const SLOT_ID: u32 = 1;
const DCBAA_ENTRY_BYTES: u64 = 8;
const DCBAA_POINTER_MASK: u64 = !0x3f;
const DEVICE_CONTEXT_POINTER_MASK: u64 = !0x3f;
const SLOT_CONTEXT_DWORD3_OFFSET: u64 = 0x0c;
const EP0_INPUT_CONTEXT_OFFSET: u64 = 0x40;
const EP0_OUTPUT_CONTEXT_OFFSET: u64 = 0x20;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x8;
const EP_TR_DEQUEUE_MASK: u64 = !0xf;
const SLOT_STATE_ADDRESSED: u32 = 3 << 27;

impl XhciController {
    pub(super) fn capture_address_device_input_context(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        input_context: u64,
        slot_id: u32,
    ) {
        if slot_id != SLOT_ID {
            return;
        }
        let input_context = input_context & DEVICE_CONTEXT_POINTER_MASK;
        let Some(ep0_dequeue_gpa) = input_context
            .checked_add(EP0_INPUT_CONTEXT_OFFSET)
            .and_then(|base| base.checked_add(EP_TR_DEQUEUE_OFFSET))
        else {
            return;
        };
        let Some(raw_dequeue) = read_mem_u64(mem, ep0_dequeue_gpa) else {
            return;
        };
        self.slot1_ep0_dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;

        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(dcbaa_entry) = u64::from(slot_id)
            .checked_mul(DCBAA_ENTRY_BYTES)
            .and_then(|slot_offset| dcbaa.checked_add(slot_offset))
        else {
            return;
        };
        let Some(output_context_raw) = read_mem_u64(mem, dcbaa_entry) else {
            return;
        };
        let output_context = output_context_raw & DEVICE_CONTEXT_POINTER_MASK;
        if output_context == 0 {
            return;
        }

        let Some(slot_dword3_gpa) = output_context.checked_add(SLOT_CONTEXT_DWORD3_OFFSET) else {
            return;
        };
        let Some(ep0_dequeue_output_gpa) = output_context
            .checked_add(EP0_OUTPUT_CONTEXT_OFFSET)
            .and_then(|base| base.checked_add(EP_TR_DEQUEUE_OFFSET))
        else {
            return;
        };
        if mem.read_bytes(slot_dword3_gpa, 4).is_none()
            || mem.read_bytes(ep0_dequeue_output_gpa, 8).is_none()
        {
            return;
        }
        if !write_mem_u32(
            mem,
            slot_dword3_gpa,
            SLOT_STATE_ADDRESSED | (slot_id & 0xff),
        ) {
            return;
        }
        if !write_mem_u64(mem, ep0_dequeue_output_gpa, raw_dequeue) {
            return;
        }
    }
}

fn read_mem_u64(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u64> {
    let raw = mem.read_bytes(gpa, 8)?;
    read_u64(&raw, 0)
}

fn write_mem_u32(mem: &mut dyn GuestMemoryMut, gpa: u64, value: u32) -> bool {
    mem.write_bytes(gpa, &value.to_le_bytes())
}

fn write_mem_u64(mem: &mut dyn GuestMemoryMut, gpa: u64, value: u64) -> bool {
    mem.write_bytes(gpa, &value.to_le_bytes())
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    let array: [u8; 8] = raw.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}
