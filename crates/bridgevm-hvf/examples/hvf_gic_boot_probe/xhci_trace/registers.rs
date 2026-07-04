pub(super) const XHCI_BAR0: usize = 0;
pub(super) const CRCR: u64 = 0x58;
pub(super) const CRCR_HI: u64 = 0x5c;
pub(super) const DCBAAP: u64 = 0x70;
pub(super) const CONFIG: u64 = 0x78;
pub(super) const ERSTSZ0: u64 = 0x1028;
pub(super) const ERSTBA0: u64 = 0x1030;
pub(super) const ERDP0: u64 = 0x1038;
pub(super) const ERDP0_HI: u64 = 0x103c;
pub(super) const DOORBELL_BASE: u64 = 0x2000;
pub(super) const DOORBELL_STRIDE: u64 = 4;
const MAX_DOORBELL_INDEX: u64 = 64;
pub(super) const COMMAND_RING_POINTER_MASK: u64 = !0x3f;
pub(super) const LINK_TRB_POINTER_MASK: u64 = !0xf;
pub(super) const DEFAULT_MAX_EVENTS: usize = 160;
pub(super) const MAX_TRANSFER_TRBS_TO_DUMP: u64 = 4;

pub(super) fn doorbell_index(offset: u64, size: u8) -> Option<u64> {
    if size != 4 || offset < DOORBELL_BASE || offset % DOORBELL_STRIDE != 0 {
        return None;
    }
    let index = (offset - DOORBELL_BASE) / DOORBELL_STRIDE;
    (index <= MAX_DOORBELL_INDEX).then_some(index)
}

pub(super) const fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}
