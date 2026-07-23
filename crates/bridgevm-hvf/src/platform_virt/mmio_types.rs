//! The MMIO access vocabulary shared with pflash and the probes.

/// A guest MMIO access as decoded from an HVF data-abort exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

/// Result of dispatching a guest MMIO access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmioOutcome {
    /// A read completed; this value is written back to the faulting register.
    ReadValue(u64),
    /// A write was accepted by a device.
    WriteAck,
    /// The address belongs to a modelled device that is not implemented yet.
    /// Carries the device name so bring-up traces are precise rather than a
    /// generic "unhandled MMIO".
    KnownUnimplemented(&'static str),
    /// The address belongs to no device in the machine map.
    Unmapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioPostDrain {
    pub(crate) xhci_setup_input_attempted: bool,
}

impl MmioPostDrain {
    pub const NONE: Self = Self {
        xhci_setup_input_attempted: false,
    };

    pub const XHCI_SETUP_INPUT: Self = Self {
        xhci_setup_input_attempted: true,
    };

    pub fn xhci_setup_input_attempted(self) -> bool {
        self.xhci_setup_input_attempted
    }
}
