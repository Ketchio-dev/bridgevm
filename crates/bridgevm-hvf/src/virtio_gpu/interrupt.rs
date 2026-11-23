//! Interrupt delivery: ISR line level, MSI-X table/PBA access, the config-change vector and
//! the GPU's MSI-X vector limit.

use super::*;
use crate::msix::{MsixMessage, MsixTable};
use crate::pcie::{
    VIRTIO_GPU_MSIX_PBA_OFFSET, VIRTIO_GPU_MSIX_TABLE_OFFSET, VIRTIO_GPU_MSIX_VECTOR_COUNT,
};

pub(crate) fn valid_msix_vector(vector: u16) -> u16 {
    if vector < VIRTIO_GPU_MSIX_VECTOR_COUNT || vector == VIRTIO_MSI_NO_VECTOR {
        vector
    } else {
        VIRTIO_MSI_NO_VECTOR
    }
}

impl VirtioGpu {
    pub fn interrupt_line_level(&self) -> bool {
        self.pci.interrupt_status != 0
    }

    /// Raised ahead of the queue vectors when a display change is pending.
    pub(crate) fn raise_config_change_msix(
        &mut self,
        msix: &mut MsixTable,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        if !self.pending_config_change {
            return;
        }
        let vector = self.pci.config_msix_vector;
        if vector == VIRTIO_MSI_NO_VECTOR {
            // No config vector programmed (INTx path): the ISR config bit
            // is already set; nothing MSI-X to raise.
            self.pending_config_change = false;
            return;
        }
        if let Some(message) = msix.raise(vector, function_enabled, function_masked) {
            self.pending_config_change = false;
            venus_start_trace_msix("config raised", vector, function_enabled, function_masked);
            out.push(message);
        } else {
            venus_start_trace_msix("config held", vector, function_enabled, function_masked);
        }
    }
}

impl VirtioPciGpu {
    pub fn interrupt_line_level(&self) -> bool {
        self.gpu.interrupt_line_level()
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciGpuOp) -> VirtioGpuResult {
        msix_bar_access(
            &mut self.msix,
            VIRTIO_GPU_MSIX_TABLE_OFFSET,
            VIRTIO_GPU_MSIX_PBA_OFFSET,
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
        self.gpu
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
        self.gpu
            .drain_msix_into(&mut self.msix, function_enabled, function_masked, out);
    }
}
