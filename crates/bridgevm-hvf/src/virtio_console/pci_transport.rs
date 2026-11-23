//! VirtioPciConsole: BAR and MSI-X access on the shared transport, forwarding to the core device.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::{MsixMessage, MsixTable};
use crate::pcie::VIRTIO_CONSOLE_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_CONSOLE_MSIX_TABLE_OFFSET;
use crate::pcie::VIRTIO_CONSOLE_MSIX_VECTOR_COUNT;

pub type VirtioConsoleResult = VirtioPciResult;

pub type VirtioPciConsoleOp = VirtioPciOp;

#[derive(Debug)]
pub struct VirtioPciConsole {
    pub(crate) console: VirtioConsole,
    pub(crate) msix: MsixTable,
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
        self.console.pci_access(offset, op, mem)
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciConsoleOp) -> VirtioConsoleResult {
        msix_bar_access(
            &mut self.msix,
            VIRTIO_CONSOLE_MSIX_TABLE_OFFSET,
            VIRTIO_CONSOLE_MSIX_PBA_OFFSET,
            offset,
            op,
        )
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.drain_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.console
            .drain_msix_into(&mut self.msix, function_enabled, function_masked, out);
    }
}

impl Default for VirtioPciConsole {
    fn default() -> Self {
        Self::new()
    }
}
