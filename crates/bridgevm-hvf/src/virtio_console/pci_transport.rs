//! VirtioPciConsole BAR region decode and forwarding to the core device.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_CONSOLE_MSIX_VECTOR_COUNT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtioConsoleResult {
    ReadValue(u64),
    WriteAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciConsoleOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

#[derive(Debug)]
pub struct VirtioPciConsole {
    pub(crate) console: VirtioConsole,
    pub(crate) msix: MsixTable,
}

pub(crate) fn common_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_COMMON_CFG_OFFSET)
}

pub(crate) fn device_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_DEVICE_CFG_OFFSET)
}

pub(crate) fn notify_queue_index(offset: u64) -> Option<u16> {
    let rel = offset.checked_sub(PCI_NOTIFY_CFG_OFFSET)?;
    (rel < PCI_CFG_REGION_SIZE).then_some((rel / 4) as u16)
}

impl VirtioPciConsole {
    pub fn new() -> Self {
        Self {
            console: VirtioConsole::new(),
            msix: MsixTable::new(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT),
        }
    }

    pub fn stats(&self) -> VirtioConsoleStats {
        self.console.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.console.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.console.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT);
    }

    pub fn agent_send(&mut self, data: &[u8]) {
        self.console.agent_send(data);
    }

    pub fn take_inbound(&mut self) -> Vec<u8> {
        self.console.take_inbound()
    }

    pub fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        self.console.drain_inbound_into(out);
    }

    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.console.poll(mem)
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciConsoleOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioConsoleResult {
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    self.console
                        .access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.console
                        .access_common(common_offset, true, size, value, mem)
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.console.config_read(device_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.console.config_write(device_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
                VirtioPciConsoleOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.console.notify_queue(queue, mem);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciConsoleOp::Read { size } => VirtioConsoleResult::ReadValue(mask_to_size(
                    u64::from(self.console.interrupt_status),
                    size,
                )),
                VirtioPciConsoleOp::Write { value, .. } => {
                    self.console.interrupt_status &= !(value as u32);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
            VirtioPciConsoleOp::Write { .. } => VirtioConsoleResult::WriteAck,
        }
    }
}

impl Default for VirtioPciConsole {
    fn default() -> Self {
        Self::new()
    }
}
