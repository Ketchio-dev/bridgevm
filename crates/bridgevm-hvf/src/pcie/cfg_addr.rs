//! ECAM address geometry and offset to bus/device/function/register decode.

/// Bytes of config space per function (PCIe extended config space: 4 KiB).
pub const CFG_SPACE_SIZE: u64 = 0x1000;

/// Functions per device (3-bit function number).
pub const FUNCS_PER_DEVICE: u8 = 8;

/// Devices per bus (5-bit device number).
pub const DEVICES_PER_BUS: u8 = 32;

// ECAM address bit layout for `pci-host-ecam-generic`:
//   addr = base + (bus << 20 | dev << 15 | fn << 12 | reg)
// i.e. 8 bits bus, 5 bits device, 3 bits function, 12 bits register.
pub(crate) const SHIFT_BUS: u64 = 20;

pub(crate) const SHIFT_DEV: u64 = 15;

pub(crate) const SHIFT_FN: u64 = 12;

pub(crate) const MASK_REG: u64 = CFG_SPACE_SIZE - 1; // low 12 bits

pub(crate) const MASK_FN: u64 = 0x7; // 3 bits

pub(crate) const MASK_DEV: u64 = 0x1f; // 5 bits

pub(crate) const MASK_BUS: u64 = 0xff; // 8 bits

/// A decoded ECAM address: which function's config space, and the register
/// offset within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CfgAddr {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    /// Register byte offset within the 4 KiB function config space.
    pub reg: u16,
}

impl CfgAddr {
    /// Decode an offset into the [`PCIE_ECAM`] window. `ecam_offset` is relative
    /// to [`PCIE_ECAM`]`.base` (the caller subtracts the base before dispatch).
    pub const fn from_ecam_offset(ecam_offset: u64) -> Self {
        Self {
            bus: ((ecam_offset >> SHIFT_BUS) & MASK_BUS) as u8,
            device: ((ecam_offset >> SHIFT_DEV) & MASK_DEV) as u8,
            function: ((ecam_offset >> SHIFT_FN) & MASK_FN) as u8,
            reg: (ecam_offset & MASK_REG) as u16,
        }
    }

    /// The Bus/Device/Function triple, for matching against modelled endpoints.
    pub const fn bdf(&self) -> (u8, u8, u8) {
        (self.bus, self.device, self.function)
    }
}
