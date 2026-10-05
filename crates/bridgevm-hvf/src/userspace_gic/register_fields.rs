//! Aligned access to distributor register banks.

use super::{mmio_regs::WriteMode, UserspaceGic};

impl UserspaceGic {
    pub(super) fn read_u32_field(registers: &[u32], base: u64, offset: u64) -> Option<u64> {
        let index = offset.checked_sub(base)? / 4;
        let aligned = (offset - base) % 4 == 0;
        (aligned && (index as usize) < registers.len())
            .then(|| u64::from(registers[index as usize]))
    }

    pub(super) fn write_u32_field(
        registers: &mut [u32],
        base: u64,
        offset: u64,
        value: u32,
        mode: WriteMode,
    ) -> bool {
        let Some(rel) = offset.checked_sub(base) else {
            return false;
        };
        if rel % 4 != 0 {
            return false;
        }
        let index = (rel / 4) as usize;
        if index >= registers.len() {
            return false;
        }
        match mode {
            WriteMode::Store => registers[index] = value,
            WriteMode::SetBits => registers[index] |= value,
            WriteMode::ClearBits => registers[index] &= !value,
        }
        true
    }
}
