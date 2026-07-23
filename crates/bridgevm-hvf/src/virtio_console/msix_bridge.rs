//! MSI-X table/PBA BAR access and pending-vector raise/drain bookkeeping.

use super::*;
use crate::msix::MsixMessage;
use crate::pcie::VIRTIO_CONSOLE_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_CONSOLE_MSIX_TABLE_OFFSET;

pub(crate) fn queue_bit(index: usize) -> Option<u8> {
    (index < u8::BITS as usize).then(|| 1u8 << index)
}

impl VirtioPciConsole {
    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciConsoleOp) -> VirtioConsoleResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
            VirtioPciConsoleOp::Write { .. } => VirtioConsoleResult::WriteAck,
        }
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
        let start = out.len();
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
        for message in &out[start..] {
            self.clear_pending_queue_for_vector(message.vector);
        }
        self.raise_pending_msix_into(function_enabled, function_masked, out);
    }

    pub(crate) fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let mut pending = self.console.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.console.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.console.queues[queue_index].pending_msix = false;
                self.console.pending_msix_queue_bits &= !(1u8 << queue_index);
                out.push(message);
            }
            pending &= !(1u8 << queue_index);
        }
    }

    pub(crate) fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.console.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.console.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_CONSOLE_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_CONSOLE_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }
}
