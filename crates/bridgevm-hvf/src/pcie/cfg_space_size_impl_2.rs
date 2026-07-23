//! Continuation of the `cfg_space_size` impl block, split for the 1000-line rule.

use super::*;

impl Function {
    /// Read a dword out of the sparse capability bytes (zero-filled).
    pub(crate) fn read_capability_dword(&self, reg: u16) -> u32 {
        let mut dword = 0u32;
        for byte in 0..4 {
            let off = reg + byte;
            if let Some(&(_, v)) = self.cap_bytes.iter().find(|&&(o, _)| o == off) {
                dword |= u32::from(v) << (byte * 8);
            }
        }
        dword
    }

    /// 32-bit dword write of register `reg`.
    pub(crate) fn write_dword(&mut self, reg: u16, value: u32) {
        match reg {
            REG_COMMAND_STATUS => {
                // Command is the low 16 bits; status (high 16) is read-only /
                // write-1-to-clear, which this model treats as ignored.
                self.command = (value as u16) & CMD_WRITABLE_MASK;
            }
            _ if (REG_BAR0..REG_BAR0 + (NUM_BARS as u16) * 4).contains(&reg) => {
                let idx = ((reg - REG_BAR0) / 4) as usize;
                self.bars[idx].write(value);
            }
            _ if self.write_capability_dword(reg, value) => {}
            // Identity, class and header registers are read-only; capability
            // bytes are read-only in this model.
            _ => {}
        }
    }

    pub(crate) fn capability_byte(&self, off: u16) -> u8 {
        self.cap_bytes
            .iter()
            .find_map(|&(o, v)| (o == off).then_some(v))
            .unwrap_or(0)
    }

    pub(crate) fn set_capability_byte(&mut self, off: u16, value: u8) {
        if let Some((_, v)) = self.cap_bytes.iter_mut().find(|(o, _)| *o == off) {
            *v = value;
        } else {
            self.cap_bytes.push((off, value));
        }
    }

    /// Handle writes into a standard MSI or MSI-X capability. Standard MSI
    /// exposes only Enable, the 64-bit message address, and Message Data as
    /// writable. MSI-X exposes only function mask and enable here; its message
    /// fields live in the device's MSI-X table BAR.
    pub(crate) fn write_capability_dword(&mut self, reg: u16, value: u32) -> bool {
        if self.write_msi_capability_dword(reg, value) {
            return true;
        }
        let Some(cap) = self.msix_capability_offset() else {
            return false;
        };
        let cap_end = cap + MsixCapability::SIZE_BYTES;
        if reg + 4 <= cap || reg >= cap_end {
            return false;
        }

        let control_off = cap + 2;
        let mut requested = u16::from_le_bytes([
            self.capability_byte(control_off),
            self.capability_byte(control_off + 1),
        ]);
        let bytes = value.to_le_bytes();
        for (byte, incoming) in bytes.iter().enumerate() {
            let off = reg + byte as u16;
            if off == control_off {
                requested = (requested & !0x00ff) | u16::from(*incoming);
            } else if off == control_off + 1 {
                requested = (requested & !0xff00) | (u16::from(*incoming) << 8);
            }
        }

        let current = u16::from_le_bytes([
            self.capability_byte(control_off),
            self.capability_byte(control_off + 1),
        ]);
        let next = (current & !0xc000) | (requested & 0xc000);
        let [lo, hi] = next.to_le_bytes();
        self.set_capability_byte(control_off, lo);
        self.set_capability_byte(control_off + 1, hi);
        true
    }

    pub(crate) fn write_msi_capability_dword(&mut self, reg: u16, value: u32) -> bool {
        let Some(cap) = self.capability_offset(CAP_ID_MSI) else {
            return false;
        };
        let cap_end = cap + HDA_MSI_CAP_BYTES.len() as u16;
        if reg + 4 <= cap || reg >= cap_end {
            return false;
        }

        for (byte, incoming) in value.to_le_bytes().into_iter().enumerate() {
            let off = reg + byte as u16;
            match off.checked_sub(cap) {
                Some(2) => {
                    let current = self.capability_byte(off);
                    self.set_capability_byte(off, (current & !0x01) | (incoming & 0x01));
                }
                Some(4) => self.set_capability_byte(off, incoming & !0x03),
                Some(5..=13) => self.set_capability_byte(off, incoming),
                _ => {}
            }
        }
        true
    }

    pub(crate) fn msi_config(&self) -> Option<HdaMsiConfig> {
        let cap = self.capability_offset(CAP_ID_MSI)?;
        let control =
            u16::from_le_bytes([self.capability_byte(cap + 2), self.capability_byte(cap + 3)]);
        let address_low = u32::from_le_bytes([
            self.capability_byte(cap + 4),
            self.capability_byte(cap + 5),
            self.capability_byte(cap + 6),
            self.capability_byte(cap + 7),
        ]);
        let address_high = u32::from_le_bytes([
            self.capability_byte(cap + 8),
            self.capability_byte(cap + 9),
            self.capability_byte(cap + 10),
            self.capability_byte(cap + 11),
        ]);
        let data = u16::from_le_bytes([
            self.capability_byte(cap + 12),
            self.capability_byte(cap + 13),
        ]);
        Some(HdaMsiConfig {
            enabled: control & 0x0001 != 0,
            address: (u64::from(address_high) << 32) | u64::from(address_low),
            data: u32::from(data),
        })
    }

    pub(crate) fn msix_control(&self) -> Option<MsixFunctionControl> {
        let control_off = self.msix_capability_offset()? + 2;
        let control = u16::from_le_bytes([
            self.capability_byte(control_off),
            self.capability_byte(control_off + 1),
        ]);
        Some(MsixFunctionControl {
            enabled: control & 0x8000 != 0,
            function_masked: control & 0x4000 != 0,
        })
    }

    pub(crate) fn msix_capability_offset(&self) -> Option<u16> {
        self.capability_offset(CAP_ID_MSIX)
    }

    pub(crate) fn capability_offset(&self, capability_id: u8) -> Option<u16> {
        let mut cap = self.cap_ptr;
        for _ in 0..32 {
            if cap == 0 {
                return None;
            }
            let off = u16::from(cap);
            if self.capability_byte(off) == capability_id {
                return Some(off);
            }
            cap = self.capability_byte(off + 1);
        }
        None
    }
}
