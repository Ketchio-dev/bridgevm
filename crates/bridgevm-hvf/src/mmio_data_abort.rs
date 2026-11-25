//! AArch64 data-abort decoding shared by the native vCPU and firmware runners.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DataAbort {
    pub size: u8,
    pub register: u32,
    pub write: bool,
    sign_extend: bool,
    register_is_64_bit: bool,
}

impl DataAbort {
    pub fn decode(syndrome: u64) -> Result<Self, String> {
        if (syndrome >> 26) & 0x3f != 0x24 || syndrome & (1 << 24) == 0 {
            return Err(format!("undecodable MMIO data abort ESR={syndrome:#x}"));
        }
        Ok(Self {
            size: 1 << ((syndrome >> 22) & 3),
            register: ((syndrome >> 16) & 0x1f) as u32,
            write: syndrome & (1 << 6) != 0,
            sign_extend: syndrome & (1 << 21) != 0,
            register_is_64_bit: syndrome & (1 << 15) != 0,
        })
    }

    pub fn read_result(self, value: u64) -> Option<u64> {
        if self.write || self.register == 31 {
            return None;
        }
        let bits = u32::from(self.size) * 8;
        let mask = u64::MAX >> (64 - bits);
        let mut value = value & mask;
        if self.sign_extend && value & (1 << (bits - 1)) != 0 {
            value |= !mask;
        }
        Some(if self.register_is_64_bit {
            value
        } else {
            value & u64::from(u32::MAX)
        })
    }
}

pub(crate) const Q_STORE_PAIR_BYTES: u64 = 32;

/// Resolve the entire AArch64 STP Q offset store on an ISV=0 translation
/// fault. Both 16-byte stores must address the same guest page.
pub(crate) fn q_store_pair_fault_ipa(
    syndrome: u64,
    word: u32,
    fault_va: u64,
    fault_ipa: u64,
    read_x: impl FnOnce(u32) -> Option<u64>,
) -> Option<u64> {
    // Lower EL, 32-bit instruction, write translation fault. The offset
    // form has no register writeback.
    if !matches!(syndrome, 0x9200_0044..=0x9200_0047) || word & 0xffc0_0000 != 0xad00_0000 {
        return None;
    }
    let register = (word >> 5) & 31;
    if register == 31 {
        return None;
    }
    let imm7 = ((word >> 15) & 0x7f) as i8;
    let displacement = i64::from(imm7 << 1 >> 1) * 16;
    let first_va = read_x(register)?.checked_add_signed(displacement)?;
    let last_va = first_va.checked_add(Q_STORE_PAIR_BYTES - 1)?;
    if first_va >> 12 != last_va >> 12
        || !(first_va..=last_va).contains(&fault_va)
        || fault_va & 0xfff != fault_ipa & 0xfff
    {
        return None;
    }
    fault_ipa.checked_sub(fault_va - first_va)
}

#[cfg(test)]
#[path = "mmio_data_abort_tests.rs"]
mod tests;
