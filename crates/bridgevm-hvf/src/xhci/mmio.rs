pub(super) fn checked_region_offset(offset: u64, base: u64, len: u64) -> Option<u64> {
    let end = base.checked_add(len)?;
    (offset >= base && offset < end).then(|| offset - base)
}

pub(super) fn merge_dword(old: u32, offset: u64, size: u8, value: u64) -> u32 {
    let shift = ((offset & 0x3) * 8) as u32;
    let width_mask: u32 = match size {
        1 => 0xff,
        2 => 0xffff,
        3 => 0x00ff_ffff,
        _ => 0xffff_ffff,
    };
    let field_mask = width_mask.checked_shl(shift).unwrap_or(0);
    let placed = ((value as u32) & width_mask)
        .checked_shl(shift)
        .unwrap_or(0);
    (old & !field_mask) | placed
}

pub(super) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}
