//! The Function config-space record and its dword read/write dispatch.

use super::*;

/// A single modelled config-space function. Today the only one is the host
/// bridge; NVMe / virtio-pci endpoints add more.
#[derive(Debug, Clone)]
pub(crate) struct Function {
    pub(crate) bdf: (u8, u8, u8),
    pub(crate) vendor_device: u32,
    pub(crate) revision_class: u32,
    pub(crate) subsystem_ids: u32,
    /// The mutable command register (low 16 bits) — bit-masked on write.
    pub(crate) command: u16,
    /// BAR latch values. A `0` size mask means "this BAR is unimplemented", so
    /// it always reads back `0` and ignores the all-ones sizing probe.
    pub(crate) bars: [Bar; NUM_BARS],
    /// Offset of the first capability in config space, or `0` for none.
    pub(crate) cap_ptr: u8,
    /// PCI Interrupt Pin byte (0 = none, 1 = INTA, ...).
    pub(crate) interrupt_pin: u8,
    /// Raw capability bytes addressed by `cap_ptr` (sparse, by byte offset).
    pub(crate) cap_bytes: Vec<(u16, u8)>,
}

impl Function {
    /// 32-bit dword read of register `reg` (already dword-aligned at the dword
    /// boundary that contains it).
    pub(crate) fn read_dword(&self, reg: u16) -> u32 {
        match reg {
            REG_VENDOR_DEVICE => self.vendor_device,
            REG_COMMAND_STATUS => {
                let status = if self.cap_ptr != 0 {
                    STATUS_CAP_LIST
                } else {
                    0
                };
                u32::from(self.command) | (u32::from(status) << 16)
            }
            REG_REVISION_CLASS => self.revision_class,
            REG_BIST_HEADER => {
                // Cache line / latency / BIST all zero; header type in byte 2.
                u32::from(HEADER_TYPE_ENDPOINT) << 16
            }
            REG_SUBSYSTEM_IDS => self.subsystem_ids,
            REG_CAP_PTR => u32::from(self.cap_ptr),
            REG_INTERRUPT_LINE_PIN => u32::from(self.interrupt_pin) << 8,
            _ if (REG_BAR0..REG_BAR0 + (NUM_BARS as u16) * 4).contains(&reg) => {
                let idx = ((reg - REG_BAR0) / 4) as usize;
                self.bars[idx].read()
            }
            _ => self.read_capability_dword(reg),
        }
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
}
