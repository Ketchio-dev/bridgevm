use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::pcie::{self, PcieMmioTarget};

use super::trb;

#[derive(Debug)]
pub(super) struct TestMemory {
    bytes: Vec<u8>,
}

impl TestMemory {
    pub(super) fn new(len: usize) -> Self {
        Self {
            bytes: vec![0; len],
        }
    }

    pub(super) fn write_u32(&mut self, gpa: u64, value: u32) {
        assert!(self.write_bytes(gpa, &value.to_le_bytes()));
    }

    pub(super) fn write_u64(&mut self, gpa: u64, value: u64) {
        assert!(self.write_bytes(gpa, &value.to_le_bytes()));
    }
}

impl GuestMemoryMut for TestMemory {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Ok(start) = usize::try_from(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(gpa).ok()?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }
}

pub(super) fn xhci_target(offset: u64) -> PcieMmioTarget {
    PcieMmioTarget {
        bdf: pcie::XHCI_BDF,
        bar_index: 0,
        offset,
    }
}

pub(super) fn command_control(trb_type: u32, slot: u32) -> u32 {
    (slot << 24) | (trb_type << trb::TYPE_SHIFT) | trb::CYCLE
}
