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

#[cfg(test)]
#[path = "mmio_data_abort_tests.rs"]
mod tests;
