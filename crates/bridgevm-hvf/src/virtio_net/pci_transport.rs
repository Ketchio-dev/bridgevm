//! VirtioPciNet: BAR and MSI-X access on the shared transport, forwarding to the core device.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::{MsixMessage, MsixTable};
use crate::pcie::VIRTIO_NET_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_NET_MSIX_TABLE_OFFSET;
use crate::pcie::VIRTIO_NET_MSIX_VECTOR_COUNT;

pub type VirtioNetResult = VirtioPciResult;

pub type VirtioPciNetOp = VirtioPciOp;

#[derive(Debug)]
pub struct VirtioPciNet<B: NetBackend = LoopbackTestBackend> {
    pub(crate) net: VirtioNet<B>,
    pub(crate) msix: MsixTable,
}

impl VirtioPciNet<LoopbackTestBackend> {
    pub fn new_loopback() -> Self {
        Self::new(LoopbackTestBackend::default())
    }
}

impl<B: NetBackend> VirtioPciNet<B> {
    pub fn new(backend: B) -> Self {
        Self {
            net: VirtioNet::new(backend),
            msix: MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT),
        }
    }

    pub fn backend(&self) -> &B {
        self.net.backend()
    }

    pub fn backend_mut(&mut self) -> &mut B {
        self.net.backend_mut()
    }

    pub fn stats(&self) -> VirtioNetStats {
        self.net.stats()
    }

    pub fn run_stats(&self) -> VirtioNetStats {
        self.net.run_stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.net.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.net.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT);
    }

    pub fn pump_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.net.pump_receive(mem)
    }

    pub fn poll_host_sockets(&mut self) {
        self.net.backend_mut().poll_host_sockets();
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciNetOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioNetResult {
        self.net.pci_access(offset, op, mem)
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciNetOp) -> VirtioNetResult {
        msix_bar_access(
            &mut self.msix,
            VIRTIO_NET_MSIX_TABLE_OFFSET,
            VIRTIO_NET_MSIX_PBA_OFFSET,
            offset,
            op,
        )
    }

    pub fn raise_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.raise_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.net
            .raise_msix_into(&mut self.msix, function_enabled, function_masked, out);
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
        self.net
            .drain_msix_into(&mut self.msix, function_enabled, function_masked, out);
    }
}
