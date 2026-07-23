//! MSI-X completion events, vector raise/drain, and MSI-X table/PBA BAR window offsets.

use super::*;
use crate::msix::MsixMessage;
use crate::pcie::NVME_MSIX_PBA_OFFSET;
use crate::pcie::NVME_MSIX_TABLE_OFFSET;

/// Completion metadata that the platform layer turns into an interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCompletionEvent {
    pub cqid: u16,
    pub vector: u16,
}

impl NvmeController {
    pub fn raise_msix(
        &mut self,
        vector: u16,
        function_enabled: bool,
        function_masked: bool,
    ) -> Option<MsixMessage> {
        self.msix.raise(vector, function_enabled, function_masked)
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        self.msix.drain_pending(function_enabled, function_masked)
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let base = u64::from(NVME_MSIX_TABLE_OFFSET);
        let rel = offset.checked_sub(base)?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let base = u64::from(NVME_MSIX_PBA_OFFSET);
        let rel = offset.checked_sub(base)?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }
}
