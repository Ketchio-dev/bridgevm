use crate::fwcfg::GuestMemoryMut;

const DCBAA_ENTRY_BYTES: u64 = 8;
const DEVICE_CONTEXT_POINTER_MASK: u64 = !0x3f;
const EP_CONTEXT_DWORD0_OFFSET: u64 = 0x0;
const EP_STATE_MASK: u32 = 0x7;

pub(super) fn output_context_for_slot(
    mem: &dyn GuestMemoryMut,
    dcbaa: u64,
    slot_id: u32,
) -> Option<u64> {
    let dcbaa_entry = u64::from(slot_id)
        .checked_mul(DCBAA_ENTRY_BYTES)
        .and_then(|slot_offset| dcbaa.checked_add(slot_offset))?;
    let output_context_raw = read_mem_u64(mem, dcbaa_entry)?;
    let output_context = output_context_raw & DEVICE_CONTEXT_POINTER_MASK;
    (output_context != 0).then_some(output_context)
}

pub(super) fn write_mem_u32(mem: &mut dyn GuestMemoryMut, gpa: u64, value: u32) -> bool {
    mem.write_bytes(gpa, &value.to_le_bytes())
}

pub(super) fn write_ep_context_state(
    mem: &mut dyn GuestMemoryMut,
    ep_context_gpa: u64,
    state: u32,
) -> bool {
    let Some(dword0_gpa) = ep_context_gpa.checked_add(EP_CONTEXT_DWORD0_OFFSET) else {
        return false;
    };
    let Some(dword0) = read_mem_u32(mem, dword0_gpa) else {
        return false;
    };
    write_mem_u32(mem, dword0_gpa, (dword0 & !EP_STATE_MASK) | state)
}

pub(super) fn read_mem_array<const N: usize>(
    mem: &dyn GuestMemoryMut,
    gpa: u64,
) -> Option<[u8; N]> {
    let mut raw = [0u8; N];
    mem.read_into(gpa, &mut raw).then_some(raw)
}

pub(super) fn read_mem_u32(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u32> {
    Some(u32::from_le_bytes(read_mem_array::<4>(mem, gpa)?))
}

pub(super) fn read_mem_u64(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u64> {
    Some(u64::from_le_bytes(read_mem_array::<8>(mem, gpa)?))
}

pub(super) fn write_mem_u64(mem: &mut dyn GuestMemoryMut, gpa: u64, value: u64) -> bool {
    mem.write_bytes(gpa, &value.to_le_bytes())
}

pub(super) fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    let array: [u8; 8] = raw.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}
