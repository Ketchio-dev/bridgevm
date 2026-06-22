use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

const SLOT_ID: u32 = 1;
const DCBAA_ENTRY_BYTES: u64 = 8;
const DCBAA_POINTER_MASK: u64 = !0x3f;
const DEVICE_CONTEXT_POINTER_MASK: u64 = !0x3f;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const SLOT_CONTEXT_DWORD3_OFFSET: u64 = 0x0c;
const EP0_INPUT_CONTEXT_OFFSET: u64 = 0x40;
const EP0_OUTPUT_CONTEXT_OFFSET: u64 = 0x20;
const DCI3: u32 = 3;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = 0x80;
const DCI3_OUTPUT_CONTEXT_OFFSET: u64 = 0x60;
const EP_CONTEXT_BYTES: usize = 32;
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
        let Some(ep0_input_gpa) = input_context.checked_add(EP0_INPUT_CONTEXT_OFFSET) else {
            return;
        };
        let Some(ep0_input_context) = mem.read_bytes(ep0_input_gpa, EP_CONTEXT_BYTES) else {
            return;
        };
        let Some(raw_dequeue) = read_u64(&ep0_input_context, EP_TR_DEQUEUE_OFFSET as usize) else {
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
        let Some(ep0_output_gpa) = output_context.checked_add(EP0_OUTPUT_CONTEXT_OFFSET) else {
            return;
        };
        let Some(ep0_dequeue_output_gpa) = ep0_output_gpa.checked_add(EP_TR_DEQUEUE_OFFSET) else {
            return;
        };
        if mem.read_bytes(slot_dword3_gpa, 4).is_none()
            || mem.read_bytes(ep0_output_gpa, EP_CONTEXT_BYTES).is_none()
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
        if !mem.write_bytes(ep0_output_gpa, &ep0_input_context) {
            return;
        }
        if !write_mem_u64(mem, ep0_dequeue_output_gpa, raw_dequeue) {
            return;
        }
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
        let Some(add_context) = input_context
            .checked_add(INPUT_CONTROL_ADD_CONTEXT_OFFSET)
            .and_then(|gpa| read_mem_u32(mem, gpa))
        else {
            return;
        };
        if add_context & (1 << DCI3) == 0 {
            return;
        }
        let Some(dci3_input_gpa) = input_context.checked_add(DCI3_INPUT_CONTEXT_OFFSET) else {
            return;
        };
        let Some(dci3_input_context) = mem.read_bytes(dci3_input_gpa, EP_CONTEXT_BYTES) else {
            return;
        };
        let Some(raw_dequeue) = read_u64(&dci3_input_context, EP_TR_DEQUEUE_OFFSET as usize) else {
            return;
        };

        let dcbaa = self.dcbaap & DCBAA_POINTER_MASK;
        let Some(output_context) = output_context_for_slot(mem, dcbaa, slot_id) else {
            return;
        };
        let Some(dci3_output_gpa) = output_context.checked_add(DCI3_OUTPUT_CONTEXT_OFFSET) else {
            return;
        };
        if mem.read_bytes(dci3_output_gpa, EP_CONTEXT_BYTES).is_none() {
            return;
        }
        if !mem.write_bytes(dci3_output_gpa, &dci3_input_context) {
            return;
        }
        self.slot1_dci3_dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
        self.slot1_dci3_dcs = raw_dequeue & 1 != 0;
    }

    pub(super) fn write_slot1_dci3_output_dequeue(&self, mem: &mut dyn GuestMemoryMut) {
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
}

fn output_context_for_slot(mem: &dyn GuestMemoryMut, dcbaa: u64, slot_id: u32) -> Option<u64> {
    let dcbaa_entry = u64::from(slot_id)
        .checked_mul(DCBAA_ENTRY_BYTES)
        .and_then(|slot_offset| dcbaa.checked_add(slot_offset))?;
    let output_context_raw = read_mem_u64(mem, dcbaa_entry)?;
    let output_context = output_context_raw & DEVICE_CONTEXT_POINTER_MASK;
    (output_context != 0).then_some(output_context)
}

fn write_mem_u32(mem: &mut dyn GuestMemoryMut, gpa: u64, value: u32) -> bool {
    mem.write_bytes(gpa, &value.to_le_bytes())
}

fn read_mem_u32(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u32> {
    let raw = mem.read_bytes(gpa, 4)?;
    read_u32(&raw, 0)
}

fn read_mem_u64(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u64> {
    let raw = mem.read_bytes(gpa, 8)?;
    read_u64(&raw, 0)
}

fn write_mem_u64(mem: &mut dyn GuestMemoryMut, gpa: u64, value: u64) -> bool {
    mem.write_bytes(gpa, &value.to_le_bytes())
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    let array: [u8; 8] = raw.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    let array: [u8; 4] = raw.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}
